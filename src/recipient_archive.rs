use crate::archive::{self, ArchiveEntry, ArchiveInfo};
use crate::keyed_stream;
use crate::recipient::{self, ArchiveKey, PasswordSlot, RecipientSlot, RgxPublicKey};
use anyhow::{anyhow, bail, Context, Result};
use std::fs::{self, File};
use std::io::{BufWriter, Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use zeroize::Zeroizing;

pub const RECIPIENT_MAGIC: [u8; 4] = *b"RGXR";
pub const RECIPIENT_VERSION: u16 = 2;
const FLAG_PASSWORD_FALLBACK: u16 = 0x0001;
const PREFIX_SIZE: usize = 16;
const RECIPIENT_SLOT_SIZE: usize = 120;
const PASSWORD_SLOT_SIZE: usize = 100;
const MAX_RECIPIENTS: usize = 64;

#[derive(Debug, Clone)]
pub struct RecipientEnvelope {
    pub recipients: Vec<RecipientSlot>,
    pub password: Option<PasswordSlot>,
    pub payload_offset: u64,
    pub envelope_hash: [u8; 32],
}

#[derive(Debug)]
pub enum UnlockMethod {
    Identity {
        path: PathBuf,
        key_id: recipient::KeyId,
    },
    PasswordFallback,
}

pub fn pack_recipient(
    input: &Path,
    output: &Path,
    level: i32,
    recipients: &[RgxPublicKey],
    fallback_password: Option<&str>,
) -> Result<ArchiveInfo> {
    if recipients.is_empty() {
        bail!("recipient-protected RGX archives require at least one recipient public key");
    }
    if recipients.len() > MAX_RECIPIENTS {
        bail!("recipient-protected RGX archives support at most {MAX_RECIPIENTS} recipients");
    }
    if output.exists() {
        bail!("output already exists: {}", output.display());
    }
    reject_output_inside_input(input, output)?;

    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.exists() {
        bail!("output directory does not exist: {}", parent.display());
    }

    let archive_key = Zeroizing::new(recipient::random_archive_key());
    let mut slots = Vec::with_capacity(recipients.len());
    for public_key in recipients {
        slots.push(recipient::wrap_archive_key_for_recipient(
            &archive_key,
            public_key,
        )?);
    }
    let password_slot = match fallback_password {
        Some(password) => Some(recipient::wrap_archive_key_with_password(
            &archive_key,
            password,
        )?),
        None => None,
    };

    let envelope_bytes = encode_envelope(&slots, password_slot.as_ref())?;
    let envelope_hash = *blake3::hash(&envelope_bytes).as_bytes();

    let mut temp =
        NamedTempFile::new_in(parent).context("failed to create recipient RGX output")?;
    let info = {
        let mut writer = BufWriter::new(temp.as_file_mut());
        writer.write_all(&envelope_bytes)?;
        let mut payload = keyed_stream::KeyedWriter::new(writer, &archive_key, envelope_hash)?;
        let info = archive::pack_to_writer(input, &mut payload, level)?;
        payload.finish()?;
        info
    };

    temp.persist(output)
        .map_err(|error| anyhow!("failed to persist recipient RGX archive: {}", error.error))?;
    Ok(info)
}

pub fn read_envelope(path: &Path) -> Result<RecipientEnvelope> {
    let mut file =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let prefix = read_exact_array::<_, PREFIX_SIZE>(&mut file)
        .context("recipient RGX header is truncated")?;
    validate_prefix(&prefix)?;

    let flags = u16::from_le_bytes(prefix[6..8].try_into().unwrap());
    let recipient_count = u16::from_le_bytes(prefix[8..10].try_into().unwrap()) as usize;
    let declared_header_len = u32::from_le_bytes(prefix[12..16].try_into().unwrap()) as usize;
    let has_password = flags & FLAG_PASSWORD_FALLBACK != 0;
    let expected_header_len = expected_header_len(recipient_count, has_password)?;
    if declared_header_len != expected_header_len {
        bail!("recipient RGX header length is inconsistent");
    }

    let file_len = file.metadata()?.len();
    if file_len <= expected_header_len as u64 + 4 {
        bail!("recipient RGX archive does not contain an encrypted payload");
    }

    file.seek(SeekFrom::Start(0))?;
    let envelope_bytes = read_exact_vec(&mut file, expected_header_len)?;
    let envelope_hash = *blake3::hash(&envelope_bytes).as_bytes();

    let mut cursor = Cursor::new(&envelope_bytes[PREFIX_SIZE..]);
    let mut recipients = Vec::with_capacity(recipient_count);
    for _ in 0..recipient_count {
        recipients.push(read_recipient_slot(&mut cursor)?);
    }
    let password = if has_password {
        Some(read_fallback_slot(&mut cursor)?)
    } else {
        None
    };
    if cursor.position() != (expected_header_len - PREFIX_SIZE) as u64 {
        bail!("recipient RGX header parsing did not consume the declared header");
    }

    file.seek(SeekFrom::Start(expected_header_len as u64))?;
    let payload_magic = read_exact_array::<_, 4>(&mut file)?;
    if payload_magic != keyed_stream::KEYED_MAGIC {
        bail!("recipient RGX payload is not a native keyed RGX stream");
    }

    Ok(RecipientEnvelope {
        recipients,
        password,
        payload_offset: expected_header_len as u64,
        envelope_hash,
    })
}

pub fn try_unlock_identity(
    path: &Path,
    explicit_identity: Option<&Path>,
) -> Result<Option<(Zeroizing<ArchiveKey>, UnlockMethod)>> {
    let envelope = read_envelope(path)?;
    let Some((key_path, private_key)) =
        recipient::find_matching_private_key(&envelope.recipients, explicit_identity)?
    else {
        return Ok(None);
    };
    let key_id = private_key.key_id();
    let slot = envelope
        .recipients
        .iter()
        .find(|slot| slot.key_id == key_id)
        .ok_or_else(|| anyhow!("matching RGX recipient slot disappeared"))?;
    let archive_key = recipient::unwrap_archive_key_for_recipient(slot, &private_key)?;
    Ok(Some((
        archive_key,
        UnlockMethod::Identity {
            path: key_path,
            key_id,
        },
    )))
}

pub fn unlock_password(
    path: &Path,
    password: &str,
) -> Result<(Zeroizing<ArchiveKey>, UnlockMethod)> {
    let envelope = read_envelope(path)?;
    let slot = envelope
        .password
        .as_ref()
        .ok_or_else(|| anyhow!("this recipient RGX archive has no password fallback"))?;
    let archive_key = recipient::unwrap_archive_key_with_password(slot, password)?;
    Ok((archive_key, UnlockMethod::PasswordFallback))
}

pub fn has_password_fallback(path: &Path) -> Result<bool> {
    Ok(read_envelope(path)?.password.is_some())
}

pub fn extract(
    path: &Path,
    output: &Path,
    selected: Option<&str>,
    archive_key: &ArchiveKey,
) -> Result<ArchiveInfo> {
    let mut reader = open_payload(path, archive_key)?;
    archive::extract_reader(&mut reader, output, selected)
}

pub fn verify(path: &Path, archive_key: &ArchiveKey) -> Result<ArchiveInfo> {
    let mut reader = open_payload(path, archive_key)?;
    archive::verify_reader(&mut reader)
}

pub fn info(path: &Path, archive_key: &ArchiveKey) -> Result<ArchiveInfo> {
    let mut reader = open_payload(path, archive_key)?;
    archive::info_reader(&mut reader)
}

pub fn list(path: &Path, archive_key: &ArchiveKey) -> Result<Vec<ArchiveEntry>> {
    let mut reader = open_payload(path, archive_key)?;
    archive::list_reader(&mut reader)
}

pub fn find(path: &Path, query: &str, archive_key: &ArchiveKey) -> Result<Vec<ArchiveEntry>> {
    let mut reader = open_payload(path, archive_key)?;
    archive::find_reader(&mut reader, query)
}

pub fn read_entry(path: &Path, entry: &str, archive_key: &ArchiveKey) -> Result<Vec<u8>> {
    let mut reader = open_payload(path, archive_key)?;
    archive::read_entry_reader(&mut reader, entry)
}

fn open_payload(path: &Path, archive_key: &ArchiveKey) -> Result<keyed_stream::KeyedReader> {
    let envelope = read_envelope(path)?;
    keyed_stream::KeyedReader::open(
        path,
        envelope.payload_offset,
        archive_key,
        envelope.envelope_hash,
    )
}

fn encode_envelope(
    recipients: &[RecipientSlot],
    password: Option<&PasswordSlot>,
) -> Result<Vec<u8>> {
    let flags = if password.is_some() {
        FLAG_PASSWORD_FALLBACK
    } else {
        0
    };
    let header_len = expected_header_len(recipients.len(), password.is_some())?;
    let header_len_u32 = u32::try_from(header_len).context("recipient RGX header is too large")?;
    let recipient_count = u16::try_from(recipients.len()).context("too many RGX recipients")?;

    let mut output = Vec::with_capacity(header_len);
    let mut prefix = [0u8; PREFIX_SIZE];
    prefix[0..4].copy_from_slice(&RECIPIENT_MAGIC);
    prefix[4..6].copy_from_slice(&RECIPIENT_VERSION.to_le_bytes());
    prefix[6..8].copy_from_slice(&flags.to_le_bytes());
    prefix[8..10].copy_from_slice(&recipient_count.to_le_bytes());
    prefix[12..16].copy_from_slice(&header_len_u32.to_le_bytes());
    output.extend_from_slice(&prefix);

    for slot in recipients {
        output.extend_from_slice(&slot.key_id);
        output.extend_from_slice(&slot.ephemeral_public);
        output.extend_from_slice(&slot.nonce);
        output.extend_from_slice(&slot.wrapped_key);
    }
    if let Some(slot) = password {
        output.extend_from_slice(&slot.memory_kib.to_le_bytes());
        output.extend_from_slice(&slot.iterations.to_le_bytes());
        output.extend_from_slice(&slot.lanes.to_le_bytes());
        output.extend_from_slice(&slot.salt);
        output.extend_from_slice(&slot.nonce);
        output.extend_from_slice(&slot.wrapped_key);
    }
    debug_assert_eq!(output.len(), header_len);
    Ok(output)
}

fn validate_prefix(prefix: &[u8; PREFIX_SIZE]) -> Result<()> {
    if prefix[0..4] != RECIPIENT_MAGIC {
        bail!("not a recipient-protected RGX archive");
    }
    let version = u16::from_le_bytes(prefix[4..6].try_into().unwrap());
    if version != RECIPIENT_VERSION {
        bail!("unsupported recipient RGX envelope version {version}");
    }
    let flags = u16::from_le_bytes(prefix[6..8].try_into().unwrap());
    if flags & !FLAG_PASSWORD_FALLBACK != 0 {
        bail!("unsupported recipient RGX envelope flags");
    }
    let recipient_count = u16::from_le_bytes(prefix[8..10].try_into().unwrap()) as usize;
    if recipient_count == 0 || recipient_count > MAX_RECIPIENTS {
        bail!("invalid recipient count in RGX envelope");
    }
    if prefix[10..12] != [0u8; 2] {
        bail!("invalid reserved recipient RGX header bytes");
    }
    Ok(())
}

fn expected_header_len(recipient_count: usize, has_password: bool) -> Result<usize> {
    PREFIX_SIZE
        .checked_add(
            recipient_count
                .checked_mul(RECIPIENT_SLOT_SIZE)
                .ok_or_else(|| anyhow!("recipient RGX header length overflow"))?,
        )
        .and_then(|value| value.checked_add(if has_password { PASSWORD_SLOT_SIZE } else { 0 }))
        .ok_or_else(|| anyhow!("recipient RGX header length overflow"))
}

fn read_exact_vec<R: Read>(reader: &mut R, len: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(len);
    let mut limited = reader.take(len as u64);
    limited.read_to_end(&mut bytes)?;
    if bytes.len() != len {
        bail!("recipient RGX field is truncated");
    }
    Ok(bytes)
}

fn read_exact_array<R: Read, const N: usize>(reader: &mut R) -> Result<[u8; N]> {
    read_exact_vec(reader, N)?
        .try_into()
        .map_err(|_| anyhow!("recipient RGX field has an unexpected length"))
}

fn read_recipient_slot<R: Read>(reader: &mut R) -> Result<RecipientSlot> {
    Ok(RecipientSlot {
        key_id: read_exact_array::<_, 16>(reader)?,
        ephemeral_public: read_exact_array::<_, 32>(reader)?,
        nonce: read_exact_array::<_, 24>(reader)?,
        wrapped_key: read_exact_array::<_, 48>(reader)?,
    })
}

fn read_fallback_slot<R: Read>(reader: &mut R) -> Result<PasswordSlot> {
    let memory_kib = read_u32(reader)?;
    let iterations = read_u32(reader)?;
    let lanes = read_u32(reader)?;
    Ok(PasswordSlot {
        memory_kib,
        iterations,
        lanes,
        salt: read_exact_array::<_, 16>(reader)?,
        nonce: read_exact_array::<_, 24>(reader)?,
        wrapped_key: read_exact_array::<_, 48>(reader)?,
    })
}

fn read_u32<R: Read>(reader: &mut R) -> Result<u32> {
    Ok(u32::from_le_bytes(read_exact_array::<_, 4>(reader)?))
}

fn reject_output_inside_input(input: &Path, output: &Path) -> Result<()> {
    if !input.is_dir() {
        return Ok(());
    }
    let input = fs::canonicalize(input)
        .with_context(|| format!("failed to canonicalize {}", input.display()))?;
    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = fs::canonicalize(parent).with_context(|| {
        format!(
            "failed to canonicalize output directory {}",
            parent.display()
        )
    })?;
    let file_name = output
        .file_name()
        .ok_or_else(|| anyhow!("output path must include a file name"))?;
    let candidate = parent.join(file_name);
    if candidate.starts_with(&input) {
        bail!("output archive must not be created inside the directory being packed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipient::RgxPrivateKey;
    use tempfile::tempdir;

    #[test]
    fn relative_output_uses_current_directory_as_parent() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir_all(&source).unwrap();
        reject_output_inside_input(&source, Path::new("recipient.rgx")).unwrap();
    }

    #[test]
    fn envelope_v2_roundtrip_preserves_slots() {
        let recipient_key = RgxPrivateKey::generate();
        let archive_key = recipient::random_archive_key();
        let recipient_slot =
            recipient::wrap_archive_key_for_recipient(&archive_key, &recipient_key.public_key())
                .unwrap();
        let password_slot = recipient::wrap_archive_key_with_password(
            &archive_key,
            &format!("rgx-envelope-test-{}", std::process::id()),
        )
        .unwrap();
        let bytes = encode_envelope(&[recipient_slot], Some(&password_slot)).unwrap();
        assert_eq!(&bytes[..4], b"RGXR");
        assert_eq!(u16::from_le_bytes(bytes[4..6].try_into().unwrap()), 2);
        assert_eq!(
            bytes.len(),
            PREFIX_SIZE + RECIPIENT_SLOT_SIZE + PASSWORD_SLOT_SIZE
        );
    }

    #[test]
    fn recipient_archive_streams_directly_and_binds_envelope() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("hello.txt"), b"native keyed recipient archive").unwrap();

        let private_path = temp.path().join("id_rgx");
        let identity = RgxPrivateKey::generate();
        let public_path = recipient::save_keypair(&private_path, &identity).unwrap();
        let public = recipient::load_public_key(&public_path).unwrap();
        let archive_path = temp.path().join("recipient.rgx");
        let fallback_password = format!("rgx-envelope-stream-test-{}", std::process::id());
        pack_recipient(
            &source,
            &archive_path,
            3,
            &[public],
            Some(&fallback_password),
        )
        .unwrap();

        let envelope = read_envelope(&archive_path).unwrap();
        assert_eq!(
            envelope.payload_offset as usize,
            PREFIX_SIZE + RECIPIENT_SLOT_SIZE + PASSWORD_SLOT_SIZE
        );
        let bytes = fs::read(&archive_path).unwrap();
        assert_eq!(
            &bytes[envelope.payload_offset as usize..envelope.payload_offset as usize + 4],
            b"RGXK"
        );

        let (identity_key, _) = try_unlock_identity(&archive_path, Some(&private_path))
            .unwrap()
            .unwrap();
        verify(&archive_path, &identity_key).unwrap();

        let output = temp.path().join("restore");
        extract(&archive_path, &output, None, &identity_key).unwrap();
        assert_eq!(
            fs::read(output.join("source/hello.txt")).unwrap(),
            b"native keyed recipient archive"
        );

        let (password_key, _) = unlock_password(&archive_path, &fallback_password).unwrap();
        assert_eq!(identity_key.as_ref(), password_key.as_ref());

        let mut tampered = bytes;
        tampered[20] ^= 0x01;
        let tampered_path = temp.path().join("tampered.rgx");
        fs::write(&tampered_path, tampered).unwrap();
        if let Ok((key, _)) = unlock_password(&tampered_path, &fallback_password) {
            assert!(verify(&tampered_path, &key).is_err());
        }
    }
}
