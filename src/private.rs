use crate::archive::{self, ArchiveEntry, ArchiveInfo};
use anyhow::{anyhow, bail, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use rand_core::{OsRng, RngCore};
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use zeroize::Zeroizing;

pub const ENCRYPTED_MAGIC: [u8; 4] = *b"RGXE";
const PLAIN_MAGIC: [u8; 4] = *b"RGX\0";
const VERSION_MAJOR: u16 = 0;
const VERSION_MINOR: u16 = 4;
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
    let mut file =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
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
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    if !parent.exists() {
        bail!("output directory does not exist: {}", parent.display());
    }

    let temp = NamedTempFile::new_in(parent).context("failed to create encrypted output file")?;
    let file = temp
        .reopen()
        .context("failed to reopen encrypted output file")?;
    let mut writer = EncryptedWriter::new(BufWriter::new(file), password)?;
    let info = archive::pack_to_writer(input, &mut writer, level)?;
    writer.finish()?;
    drop(writer);
    temp.persist(output)
        .map_err(|error| anyhow!("failed to persist encrypted archive: {}", error.error))?;
    Ok(info)
}

pub fn extract_private(path: &Path, output: &Path, password: &str) -> Result<ArchiveInfo> {
    let mut reader = EncryptedReader::open(path, password)?;
    archive::extract_reader(&mut reader, output, None)
}

pub fn extract_selected_private(
    path: &Path,
    output: &Path,
    selected: &str,
    password: &str,
) -> Result<ArchiveInfo> {
    let mut reader = EncryptedReader::open(path, password)?;
    archive::extract_reader(&mut reader, output, Some(selected))
}

pub fn verify_private(path: &Path, password: &str) -> Result<ArchiveInfo> {
    let mut reader = EncryptedReader::open(path, password)?;
    archive::verify_reader(&mut reader)
}

pub fn info_private(path: &Path, password: &str) -> Result<ArchiveInfo> {
    let mut reader = EncryptedReader::open(path, password)?;
    archive::info_reader(&mut reader)
}

pub fn list_private(path: &Path, password: &str) -> Result<Vec<ArchiveEntry>> {
    let mut reader = EncryptedReader::open(path, password)?;
    archive::list_reader(&mut reader)
}

pub fn find_private(path: &Path, query: &str, password: &str) -> Result<Vec<ArchiveEntry>> {
    let mut reader = EncryptedReader::open(path, password)?;
    archive::find_reader(&mut reader, query)
}

pub fn read_entry_private(path: &Path, entry: &str, password: &str) -> Result<Vec<u8>> {
    let mut reader = EncryptedReader::open(path, password)?;
    archive::read_entry_reader(&mut reader, entry)
}

struct EncryptedWriter<W: Write> {
    writer: W,
    header: EncryptionHeader,
    header_bytes: [u8; HEADER_SIZE],
    cipher: XChaCha20Poly1305,
    buffer: Vec<u8>,
    sequence: u64,
    finished: bool,
}

impl<W: Write> EncryptedWriter<W> {
    fn new(writer: W, password: &str) -> Result<Self> {
        Self::new_with_minor(writer, password, VERSION_MINOR)
    }

    fn new_with_minor(mut writer: W, password: &str, minor: u16) -> Result<Self> {
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
        let header_bytes = encode_header_with_minor(&header, minor);
        writer.write_all(&header_bytes)?;
        let key = derive_key(password, &header)?;
        let cipher = XChaCha20Poly1305::new_from_slice(key.as_ref())
            .map_err(|_| anyhow!("failed to initialize XChaCha20-Poly1305"))?;
        let capacity = header.frame_size as usize;
        Ok(Self {
            writer,
            header,
            header_bytes,
            cipher,
            buffer: Vec::with_capacity(capacity),
            sequence: 0,
            finished: false,
        })
    }

    fn emit(&mut self, last: bool) -> Result<()> {
        write_encrypted_frame(
            &mut self.writer,
            &self.cipher,
            &self.header,
            &self.header_bytes,
            self.sequence,
            &self.buffer,
            last,
        )?;
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| anyhow!("private RGX frame sequence overflow"))?;
        self.buffer.clear();
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        if !self.finished {
            self.emit(true)?;
            self.writer.flush()?;
            self.finished = true;
        }
        Ok(())
    }
}

impl<W: Write> Write for EncryptedWriter<W> {
    fn write(&mut self, mut data: &[u8]) -> io::Result<usize> {
        if self.finished {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "private writer is finished",
            ));
        }
        let original = data.len();
        while !data.is_empty() {
            let available = self.header.frame_size as usize - self.buffer.len();
            let take = available.min(data.len());
            self.buffer.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.buffer.len() == self.header.frame_size as usize {
                self.emit(false).map_err(to_io_error)?;
            }
        }
        Ok(original)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

struct EncryptedReader {
    file: File,
    header: EncryptionHeader,
    header_bytes: [u8; HEADER_SIZE],
    cipher: XChaCha20Poly1305,
    position: u64,
    plaintext_len: u64,
    final_sequence: u64,
    cached_sequence: Option<u64>,
    cached_plaintext: Vec<u8>,
}

impl EncryptedReader {
    fn open(path: &Path, password: &str) -> Result<Self> {
        validate_password(password)?;
        let mut file =
            File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
        let mut header_bytes = [0u8; HEADER_SIZE];
        file.read_exact(&mut header_bytes)
            .context("encrypted RGX header is truncated")?;
        let header = decode_header(&header_bytes)?;
        let key = derive_key(password, &header)?;
        let cipher = XChaCha20Poly1305::new_from_slice(key.as_ref())
            .map_err(|_| anyhow!("failed to initialize XChaCha20-Poly1305"))?;

        let file_len = file.metadata()?.len();
        let stride = FRAME_HEADER_SIZE as u64 + header.frame_size as u64 + TAG_SIZE as u64;
        let mut sequence = 0u64;
        let (plaintext_len, final_sequence) = loop {
            let offset = HEADER_SIZE as u64
                + sequence
                    .checked_mul(stride)
                    .ok_or_else(|| anyhow!("private RGX frame offset overflow"))?;
            if offset + FRAME_HEADER_SIZE as u64 > file_len {
                bail!("encrypted RGX archive ended before its final frame");
            }
            file.seek(SeekFrom::Start(offset))?;
            let mut frame = [0u8; FRAME_HEADER_SIZE];
            file.read_exact(&mut frame)?;
            let (actual, plain, cipher_len, last) = decode_frame_header(&frame, &header)?;
            if actual != sequence {
                bail!("encrypted RGX frame sequence mismatch");
            }
            let end = offset + FRAME_HEADER_SIZE as u64 + cipher_len as u64;
            if end > file_len {
                bail!("encrypted RGX frame payload is truncated");
            }
            if last {
                if end != file_len {
                    bail!("encrypted RGX archive contains trailing data after final frame");
                }
                let total = sequence
                    .checked_mul(header.frame_size as u64)
                    .and_then(|value| value.checked_add(plain as u64))
                    .ok_or_else(|| anyhow!("private RGX plaintext length overflow"))?;
                break (total, sequence);
            }
            sequence = sequence
                .checked_add(1)
                .ok_or_else(|| anyhow!("private RGX frame sequence overflow"))?;
        };

        Ok(Self {
            file,
            header,
            header_bytes,
            cipher,
            position: 0,
            plaintext_len,
            final_sequence,
            cached_sequence: None,
            cached_plaintext: Vec::new(),
        })
    }

    fn load_frame(&mut self, sequence: u64) -> Result<()> {
        if self.cached_sequence == Some(sequence) {
            return Ok(());
        }
        if sequence > self.final_sequence {
            bail!("private RGX seek is outside the plaintext stream");
        }
        let stride = FRAME_HEADER_SIZE as u64 + self.header.frame_size as u64 + TAG_SIZE as u64;
        let offset = HEADER_SIZE as u64
            + sequence
                .checked_mul(stride)
                .ok_or_else(|| anyhow!("private RGX frame offset overflow"))?;
        self.file.seek(SeekFrom::Start(offset))?;
        let mut frame = [0u8; FRAME_HEADER_SIZE];
        self.file.read_exact(&mut frame)?;
        let (actual, plaintext_len, ciphertext_len, _) = decode_frame_header(&frame, &self.header)?;
        if actual != sequence {
            bail!("encrypted RGX frame sequence mismatch");
        }
        let mut ciphertext = vec![0u8; ciphertext_len as usize];
        self.file.read_exact(&mut ciphertext)?;
        let nonce = frame_nonce(&self.header.nonce_prefix, sequence);
        let aad = frame_aad(&self.header_bytes, &frame);
        let plaintext = self
            .cipher
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| {
                anyhow!("private RGX authentication failed (wrong password or damaged archive)")
            })?;
        if plaintext.len() != plaintext_len as usize {
            bail!("private RGX frame length verification failed");
        }
        self.cached_plaintext = plaintext;
        self.cached_sequence = Some(sequence);
        Ok(())
    }
}

impl Read for EncryptedReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() || self.position >= self.plaintext_len {
            return Ok(0);
        }
        let mut written = 0usize;
        while written < output.len() && self.position < self.plaintext_len {
            let sequence = self.position / self.header.frame_size as u64;
            let within = (self.position % self.header.frame_size as u64) as usize;
            self.load_frame(sequence).map_err(to_io_error)?;
            let available = self.cached_plaintext.len().saturating_sub(within);
            if available == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "invalid private frame length",
                ));
            }
            let remaining = output.len() - written;
            let take = available.min(remaining);
            output[written..written + take]
                .copy_from_slice(&self.cached_plaintext[within..within + take]);
            written += take;
            self.position += take as u64;
        }
        Ok(written)
    }
}

impl Seek for EncryptedReader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let next = match from {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::Current(value) => i128::from(self.position) + i128::from(value),
            SeekFrom::End(value) => i128::from(self.plaintext_len) + i128::from(value),
        };
        if next < 0 || next > i128::from(u64::MAX) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid private RGX seek",
            ));
        }
        self.position = next as u64;
        Ok(self.position)
    }
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
    writer.write_all(&frame_header)?;
    writer.write_all(&ciphertext)?;
    Ok(())
}

fn derive_key(password: &str, header: &EncryptionHeader) -> Result<Zeroizing<[u8; 32]>> {
    let params = Params::new(header.memory_kib, header.iterations, header.lanes, Some(32))
        .map_err(|error| anyhow!("invalid Argon2 parameters: {error}"))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password.as_bytes(), &header.salt, key.as_mut())
        .map_err(|error| anyhow!("Argon2id key derivation failed: {error}"))?;
    Ok(key)
}

fn encode_header_with_minor(header: &EncryptionHeader, minor: u16) -> [u8; HEADER_SIZE] {
    let mut bytes = [0u8; HEADER_SIZE];
    bytes[0..4].copy_from_slice(&ENCRYPTED_MAGIC);
    bytes[4..6].copy_from_slice(&VERSION_MAJOR.to_le_bytes());
    bytes[6..8].copy_from_slice(&minor.to_le_bytes());
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
    if major != VERSION_MAJOR || !matches!(minor, 3 | VERSION_MINOR) {
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

fn frame_aad(header: &[u8; HEADER_SIZE], frame_header: &[u8; FRAME_HEADER_SIZE]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(HEADER_SIZE + FRAME_HEADER_SIZE);
    aad.extend_from_slice(header);
    aad.extend_from_slice(frame_header);
    aad
}

fn validate_kdf_parameters(
    memory_kib: u32,
    iterations: u32,
    lanes: u32,
    frame_size: u32,
) -> Result<()> {
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

fn to_io_error(error: anyhow::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;
    use std::io::Read;
    use tempfile::tempdir;

    const PASSWORD: &str = "RGX v0.3 compatibility fixture password";

    #[test]
    fn reads_v03_private_envelope() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("fixture.txt");
        fs::write(&source, b"RGX v0.3 compatibility fixture").unwrap();
        let archive_path = temp.path().join("fixture-v03.rgx");

        let file = File::create(&archive_path).unwrap();
        let mut writer =
            EncryptedWriter::new_with_minor(BufWriter::new(file), PASSWORD, 3).unwrap();
        archive::pack_to_writer(&source, &mut writer, 3).unwrap();
        writer.finish().unwrap();
        drop(writer);

        let bytes = fs::read(&archive_path).unwrap();
        assert_eq!(u16::from_le_bytes(bytes[6..8].try_into().unwrap()), 3);
        assert_eq!(list_private(&archive_path, PASSWORD).unwrap().len(), 1);
        verify_private(&archive_path, PASSWORD).unwrap();
        assert_eq!(
            read_entry_private(&archive_path, "fixture.txt", PASSWORD).unwrap(),
            b"RGX v0.3 compatibility fixture"
        );
    }

    #[test]
    fn seekable_reader_handles_exact_frame_boundary() {
        let temp = tempdir().unwrap();
        let archive_path = temp.path().join("frame-boundary.rgx");
        let plaintext = vec![0x5au8; DEFAULT_FRAME_SIZE as usize];

        let file = File::create(&archive_path).unwrap();
        let mut writer = EncryptedWriter::new(BufWriter::new(file), PASSWORD).unwrap();
        writer.write_all(&plaintext).unwrap();
        writer.finish().unwrap();
        drop(writer);

        let mut reader = EncryptedReader::open(&archive_path, PASSWORD).unwrap();
        reader
            .seek(SeekFrom::Start(DEFAULT_FRAME_SIZE as u64 - 16))
            .unwrap();
        let mut tail = Vec::new();
        reader.read_to_end(&mut tail).unwrap();
        assert_eq!(tail, plaintext[plaintext.len() - 16..]);
    }
}
