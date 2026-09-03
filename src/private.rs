use crate::archive::{self, ArchiveEntry, ArchiveInfo};
use anyhow::{anyhow, bail, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use rand_core::{OsRng, RngCore};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use tempfile::{tempdir, NamedTempFile};
use zeroize::Zeroizing;

pub const ENCRYPTED_MAGIC: [u8; 4] = *b"RGXE";
const PLAIN_MAGIC: [u8; 4] = *b"RGX\0";
const VERSION_MAJOR: u16 = 0;
const VERSION_MINOR: u16 = 3;
const KDF_ARGON2ID: u8 = 1;
const AEAD_XCHACHA20_POLY1305: u8 = 1;
const SALT_SIZE: usize = 16;
const NONCE_PREFIX_SIZE: usize = 16;
const HEADER_SIZE: usize = 60;
const FRAME_MAGIC: [u8; 4] = *b"FRAM";
const FRAME_HEADER_SIZE: usize = 24;
const TAG_SIZE: usize = 16;
const DEFAULT_FRAME_SIZE: u32 = 1024 * 1024;
const ARGON2_MEMORY_KIB: u32 = 64 * 1024;
const ARGON2_ITERATIONS: u32 = 3;
const ARGON2_LANES: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Plain,
    Private,
}

#[derive(Debug, Clone)]
struct EncryptionHeader {
    memory_kib: u32,
    iterations: u32,
    lanes: u32,
    frame_size: u32,
    salt: [u8; SALT_SIZE],
    nonce_prefix: [u8; NONCE_PREFIX_SIZE],
}

pub fn detect_kind(path: &Path) -> Result<ArchiveKind> {
    let mut file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;
    match magic {
        ENCRYPTED_MAGIC => Ok(ArchiveKind::Private),
        PLAIN_MAGIC => Ok(ArchiveKind::Plain),
        _ => bail!("not a recognized RGX archive"),
    }
}

pub fn pack_private(
    input: &Path,
    output: &Path,
    level: i32,
    password: &str,
) -> Result<ArchiveInfo> {
    validate_password(password)?;
    if output.exists() {
        bail!("output already exists: {}", output.display());
    }
    reject_output_inside_input(input, output)?;

    let temp = tempdir().context("failed to create private RGX working directory")?;
    let inner = temp.path().join("inner.rgx");
    let info = archive::pack(input, &inner, level)?;
    encrypt_file(&inner, output, password)?;
    Ok(info)
}

pub fn extract_private(archive_path: &Path, output: &Path, password: &str) -> Result<ArchiveInfo> {
    validate_password(password)?;
    if output.exists() {
        bail!("output already exists: {}", output.display());
    }

    with_decrypted_archive(archive_path, password, |inner| archive::extract(inner, output))
}

pub fn verify_private(archive_path: &Path, password: &str) -> Result<ArchiveInfo> {
    validate_password(password)?;
    with_decrypted_archive(archive_path, password, archive::verify)
}

pub fn info_private(archive_path: &Path, password: &str) -> Result<ArchiveInfo> {
    validate_password(password)?;
    with_decrypted_archive(archive_path, password, archive::info)
}

pub fn list_private(archive_path: &Path, password: &str) -> Result<Vec<ArchiveEntry>> {
    validate_password(password)?;
    with_decrypted_archive(archive_path, password, archive::list)
}

fn with_decrypted_archive<T, F>(archive_path: &Path, password: &str, operation: F) -> Result<T>
where
    F: FnOnce(&Path) -> Result<T>,
{
    let temp = tempdir().context("failed to create private RGX working directory")?;
    let inner = temp.path().join("inner.rgx");
    decrypt_file(archive_path, &inner, password)?;
    operation(&inner)
}

fn encrypt_file(input: &Path, output: &Path, password: &str) -> Result<()> {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    if !parent.exists() {
        bail!("output directory does not exist: {}", parent.display());
    }
    if output.file_name().is_none() {
        bail!("output path must include a file name");
    }

    let mut salt = [0u8; SALT_SIZE];
    let mut nonce_prefix = [0u8; NONCE_PREFIX_SIZE];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce_prefix);

    let header = EncryptionHeader {
        memory_kib: ARGON2_MEMORY_KIB,
        iterations: ARGON2_ITERATIONS,
        lanes: ARGON2_LANES,
        frame_size: DEFAULT_FRAME_SIZE,
        salt,
        nonce_prefix,
    };
    let header_bytes = encode_header(&header);
    let key = derive_key(password, &header)?;
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_ref())
        .map_err(|_| anyhow!("failed to initialize XChaCha20-Poly1305"))?;

    let mut reader = BufReader::new(
        File::open(input).with_context(|| format!("failed to open {}", input.display()))?,
    );
    let total = fs::metadata(input)?.len();
    let mut written_plaintext = 0u64;
    let mut sequence = 0u64;
    let mut buffer = vec![0u8; header.frame_size as usize];

    let temp = NamedTempFile::new_in(parent).context("failed to create encrypted output file")?;
    let output_file = temp.reopen().context("failed to reopen encrypted output file")?;
    let mut writer = BufWriter::new(output_file);
    writer.write_all(&header_bytes)?;

    if total == 0 {
        write_encrypted_frame(&mut writer, &cipher, &header, &header_bytes, sequence, &[], true)?;
    } else {
        while written_plaintext < total {
            let remaining = total - written_plaintext;
            let wanted = usize::try_from(remaining.min(header.frame_size as u64))?;
            reader.read_exact(&mut buffer[..wanted])?;
            written_plaintext += wanted as u64;
            let last = written_plaintext == total;
            write_encrypted_frame(
                &mut writer,
                &cipher,
                &header,
                &header_bytes,
                sequence,
                &buffer[..wanted],
                last,
            )?;
            sequence = sequence
                .checked_add(1)
                .ok_or_else(|| anyhow!("private RGX frame sequence overflow"))?;
        }
    }

    writer.flush()?;
    writer.get_ref().sync_all()?;
    drop(writer);
    temp.persist(output)
        .map_err(|error| anyhow!("failed to persist encrypted archive: {}", error.error))?;
    Ok(())
}

fn decrypt_file(input: &Path, output: &Path, password: &str) -> Result<()> {
    if output.exists() {
        bail!("temporary decrypted archive already exists");
    }

    let mut reader = BufReader::new(
        File::open(input).with_context(|| format!("failed to open {}", input.display()))?,
    );
    let mut header_bytes = [0u8; HEADER_SIZE];
    reader
        .read_exact(&mut header_bytes)
        .context("encrypted RGX header is truncated")?;
    let header = decode_header(&header_bytes)?;
    let key = derive_key(password, &header)?;
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_ref())
        .map_err(|_| anyhow!("failed to initialize XChaCha20-Poly1305"))?;

    let mut writer = BufWriter::new(
        File::create(output).with_context(|| format!("failed to create {}", output.display()))?,
    );
    let result = (|| -> Result<()> {
        let mut expected_sequence = 0u64;
        loop {
            let mut frame_bytes = [0u8; FRAME_HEADER_SIZE];
            reader
                .read_exact(&mut frame_bytes)
                .context("encrypted RGX archive ended before its final frame")?;
            let (sequence, plaintext_len, ciphertext_len, last) =
                decode_frame_header(&frame_bytes, &header)?;
            if sequence != expected_sequence {
                bail!("encrypted RGX frame sequence mismatch");
            }

            let mut ciphertext = vec![0u8; ciphertext_len as usize];
            reader
                .read_exact(&mut ciphertext)
                .context("encrypted RGX frame payload is truncated")?;
            let nonce = frame_nonce(&header.nonce_prefix, sequence);
            let aad = frame_aad(&header_bytes, &frame_bytes);
            let plaintext = cipher
                .decrypt(
                    XNonce::from_slice(&nonce),
                    Payload {
                        msg: &ciphertext,
                        aad: &aad,
                    },
                )
                .map_err(|_| anyhow!("private RGX authentication failed (wrong password or damaged archive)"))?;
            if plaintext.len() != plaintext_len as usize {
                bail!("private RGX frame length verification failed");
            }
            writer.write_all(&plaintext)?;

            if last {
                let mut trailing = [0u8; 1];
                if reader.read(&mut trailing)? != 0 {
                    bail!("encrypted RGX archive contains trailing data after final frame");
                }
                break;
            }
            expected_sequence = expected_sequence
                .checked_add(1)
                .ok_or_else(|| anyhow!("private RGX frame sequence overflow"))?;
        }
        writer.flush()?;
        Ok(())
    })();

    if result.is_err() {
        drop(writer);
        let _ = fs::remove_file(output);
    }
    result
}

fn write_encrypted_frame<W: Write>(
    writer: &mut W,
    cipher: &XChaCha20Poly1305,
    header: &EncryptionHeader,
    header_bytes: &[u8; HEADER_SIZE],
    sequence: u64,
    plaintext: &[u8],
    last: bool,
) -> Result<()> {
    let plaintext_len = u32::try_from(plaintext.len()).context("private RGX frame is too large")?;
    if plaintext_len > header.frame_size {
        bail!("private RGX frame exceeds configured frame size");
    }
    let ciphertext_len = plaintext_len
        .checked_add(TAG_SIZE as u32)
        .ok_or_else(|| anyhow!("private RGX ciphertext length overflow"))?;
    let frame_header = encode_frame_header(sequence, plaintext_len, ciphertext_len, last);
    let nonce = frame_nonce(&header.nonce_prefix, sequence);
    let aad = frame_aad(header_bytes, &frame_header);
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| anyhow!("XChaCha20-Poly1305 encryption failed"))?;
    if ciphertext.len() != ciphertext_len as usize {
        bail!("private RGX encryption produced an unexpected frame size");
    }

    writer.write_all(&frame_header)?;
    writer.write_all(&ciphertext)?;
    Ok(())
}

fn derive_key(password: &str, header: &EncryptionHeader) -> Result<Zeroizing<[u8; 32]>> {
    let params = Params::new(
        header.memory_kib,
        header.iterations,
        header.lanes,
        Some(32),
    )
    .map_err(|error| anyhow!("invalid Argon2 parameters: {error}"))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password.as_bytes(), &header.salt, key.as_mut())
        .map_err(|error| anyhow!("Argon2id key derivation failed: {error}"))?;
    Ok(key)
}

fn encode_header(header: &EncryptionHeader) -> [u8; HEADER_SIZE] {
    let mut bytes = [0u8; HEADER_SIZE];
    bytes[0..4].copy_from_slice(&ENCRYPTED_MAGIC);
    bytes[4..6].copy_from_slice(&VERSION_MAJOR.to_le_bytes());
    bytes[6..8].copy_from_slice(&VERSION_MINOR.to_le_bytes());
    bytes[8] = KDF_ARGON2ID;
    bytes[9] = AEAD_XCHACHA20_POLY1305;
    bytes[12..16].copy_from_slice(&header.memory_kib.to_le_bytes());
    bytes[16..20].copy_from_slice(&header.iterations.to_le_bytes());
    bytes[20..24].copy_from_slice(&header.lanes.to_le_bytes());
    bytes[24..28].copy_from_slice(&header.frame_size.to_le_bytes());
    bytes[28..44].copy_from_slice(&header.salt);
    bytes[44..60].copy_from_slice(&header.nonce_prefix);
    bytes
}

fn decode_header(bytes: &[u8; HEADER_SIZE]) -> Result<EncryptionHeader> {
    if bytes[0..4] != ENCRYPTED_MAGIC {
        bail!("not a private RGX archive");
    }
    let major = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
    let minor = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
    if major != VERSION_MAJOR || minor != VERSION_MINOR {
        bail!("unsupported private RGX envelope version {major}.{minor}");
    }
    if bytes[8] != KDF_ARGON2ID || bytes[9] != AEAD_XCHACHA20_POLY1305 {
        bail!("unsupported private RGX cryptographic profile");
    }
    if bytes[10] != 0 || bytes[11] != 0 {
        bail!("unsupported private RGX header flags");
    }

    let memory_kib = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    let iterations = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    let lanes = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
    let frame_size = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    validate_kdf_parameters(memory_kib, iterations, lanes, frame_size)?;

    let mut salt = [0u8; SALT_SIZE];
    salt.copy_from_slice(&bytes[28..44]);
    let mut nonce_prefix = [0u8; NONCE_PREFIX_SIZE];
    nonce_prefix.copy_from_slice(&bytes[44..60]);

    Ok(EncryptionHeader {
        memory_kib,
        iterations,
        lanes,
        frame_size,
        salt,
        nonce_prefix,
    })
}

fn encode_frame_header(
    sequence: u64,
    plaintext_len: u32,
    ciphertext_len: u32,
    last: bool,
) -> [u8; FRAME_HEADER_SIZE] {
    let mut bytes = [0u8; FRAME_HEADER_SIZE];
    bytes[0..4].copy_from_slice(&FRAME_MAGIC);
    bytes[4] = u8::from(last);
    bytes[8..16].copy_from_slice(&sequence.to_le_bytes());
    bytes[16..20].copy_from_slice(&plaintext_len.to_le_bytes());
    bytes[20..24].copy_from_slice(&ciphertext_len.to_le_bytes());
    bytes
}

fn decode_frame_header(
    bytes: &[u8; FRAME_HEADER_SIZE],
    header: &EncryptionHeader,
) -> Result<(u64, u32, u32, bool)> {
    if bytes[0..4] != FRAME_MAGIC {
        bail!("invalid private RGX frame marker");
    }
    if bytes[4] > 1 || bytes[5..8] != [0u8; 3] {
        bail!("invalid private RGX frame flags");
    }
    let last = bytes[4] == 1;
    let sequence = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    let plaintext_len = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    let ciphertext_len = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
    if plaintext_len > header.frame_size {
        bail!("private RGX frame declares too much plaintext");
    }
    if !last && plaintext_len != header.frame_size {
        bail!("non-final private RGX frame is not full sized");
    }
    if ciphertext_len != plaintext_len + TAG_SIZE as u32 {
        bail!("private RGX frame ciphertext length is invalid");
    }
    Ok((sequence, plaintext_len, ciphertext_len, last))
}

fn frame_nonce(prefix: &[u8; NONCE_PREFIX_SIZE], sequence: u64) -> [u8; 24] {
    let mut nonce = [0u8; 24];
    nonce[..NONCE_PREFIX_SIZE].copy_from_slice(prefix);
    nonce[NONCE_PREFIX_SIZE..].copy_from_slice(&sequence.to_le_bytes());
    nonce
}

fn frame_aad(
    header: &[u8; HEADER_SIZE],
    frame_header: &[u8; FRAME_HEADER_SIZE],
) -> Vec<u8> {
    let mut aad = Vec::with_capacity(HEADER_SIZE + FRAME_HEADER_SIZE);
    aad.extend_from_slice(header);
    aad.extend_from_slice(frame_header);
    aad
}

fn validate_kdf_parameters(memory_kib: u32, iterations: u32, lanes: u32, frame_size: u32) -> Result<()> {
    if !(8 * 1024..=1024 * 1024).contains(&memory_kib) {
        bail!("private RGX Argon2 memory parameter is outside the accepted range");
    }
    if !(1..=10).contains(&iterations) {
        bail!("private RGX Argon2 iteration parameter is outside the accepted range");
    }
    if !(1..=16).contains(&lanes) {
        bail!("private RGX Argon2 lane parameter is outside the accepted range");
    }
    if !(64 * 1024..=4 * 1024 * 1024).contains(&frame_size) {
        bail!("private RGX frame size is outside the accepted range");
    }
    Ok(())
}

fn validate_password(password: &str) -> Result<()> {
    if password.is_empty() {
        bail!("private RGX password must not be empty");
    }
    Ok(())
}

fn reject_output_inside_input(input: &Path, output: &Path) -> Result<()> {
    if !input.is_dir() {
        return Ok(());
    }
    let input = fs::canonicalize(input)
        .with_context(|| format!("failed to canonicalize {}", input.display()))?;
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let parent = fs::canonicalize(parent).with_context(|| {
        format!(
            "failed to canonicalize output directory {}",
            parent.display()
        )
    })?;
    let file_name = output
        .file_name()
        .ok_or_else(|| anyhow!("output path must include a file name"))?;
    let candidate: PathBuf = parent.join(file_name);
    if candidate.starts_with(&input) {
        bail!("output archive must not be created inside the directory being packed");
    }
    Ok(())
}
