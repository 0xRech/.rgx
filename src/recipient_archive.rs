use crate::archive::{ArchiveEntry, ArchiveInfo};
use crate::private;
use crate::recipient::{
    self, ArchiveKey, PasswordSlot, RecipientSlot, RgxPrivateKey, RgxPublicKey,
};
use anyhow::{anyhow, bail, Context, Result};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tempfile::{tempdir_in, NamedTempFile};
use zeroize::Zeroizing;

pub const RECIPIENT_MAGIC: [u8; 4] = *b"RGXR";
const VERSION: u16 = 1;
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
}

#[derive(Debug)]
pub enum UnlockMethod {
    Identity { path: PathBuf, key_id: recipient::KeyId },
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
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    if !parent.exists() {
        bail!("output directory does not exist: {}", parent.display());
    }

    let archive_key = Zeroizing::new(recipient::random_archive_key());
    let mut slots = Vec::with_capacity(recipients.len());
    for public_key in recipients {
        slots.push(recipient::wrap_archive_key_for_recipient(
            archive_key.as_ref(),
            public_key,
        )?);
    }
    let password_slot = match fallback_password {
        Some(password) => Some(recipient::wrap_archive_key_with_password(
            archive_key.as_ref(),
            password,
        )?),
        None => None,
    };

    let workspace = tempdir_in(parent).context("failed to create recipient archive workspace")?;
    let inner_path = workspace.path().join("payload.rgx");
    let secret = archive_key_password(archive_key.as_ref());
    let info = private::pack_private(input, &inner_path, level, secret.as_str())?;

    let mut temp = NamedTempFile::new_in(parent).context("failed to create recipient RGX output")?;
    {
        let mut writer = BufWriter::new(temp.as_file_mut());
        write_envelope(&mut writer, &slots, password_slot.as_ref())?;
        let mut inner = BufReader::new(File::open(&inner_path)?);
        std::io::copy(&mut inner, &mut writer)?;
        writer.flush()?;
    }
    temp.persist(output)
        .map_err(|error| anyhow!("failed to persist recipient RGX archive: {}", error.error))?;
    Ok(info)
}

pub fn read_envelope(path: &Path) -> Result<RecipientEnvelope> {
    let mut file = File::open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    let mut prefix = [0u8; PREFIX_SIZE];
    file.read_exact(&mut prefix)
        .context("recipient RGX header is truncated")?;
    if prefix[0..4] != RECIPIENT_MAGIC {
        bail!("not a recipient-protected RGX archive");
    }
    let version = u16::from_le_bytes(prefix[4..6].try_into().unwrap());
    if version != VERSION {
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
    let declared_header_len = u32::from_le_bytes(prefix[12..16].try_into().unwrap()) as usize;
    let has_password = flags & FLAG_PASSWORD_FALLBACK != 0;
    let expected_header_len = PREFIX_SIZE
        .checked_add(recipient_count * RECIPIENT_SLOT_SIZE)
        .and_then(|value| value.checked_add(if has_password { PASSWORD_SLOT_SIZE } else { 0 }))
        .ok_or_else(|| anyhow!("recipient RGX header length overflow"))?;
    if declared_header_len != expected_header_len {
        bail!("recipient RGX header length is inconsistent");
    }

    let mut recipients = Vec::with_capacity(recipient_count);
    for _ in 0..recipient_count {
        recipients.push(read_recipient_slot(&mut file)?);
    }
    let password = if has_password {
        Some(read_password_slot(&mut file)?)
    } else {
        None
    };

    let file_len = file.metadata()?.len();
    if file_len <= expected_header_len as u64 + 4 {
        bail!("recipient RGX archive does not contain an encrypted payload");
    }
    file.seek(SeekFrom::Start(expected_header_len as u64))?;
    let mut inner_magic = [0u8; 4];
    file.read_exact(&mut inner_magic)?;
    if inner_magic != private::ENCRYPTED_MAGIC {
        bail!("recipient RGX payload is not a private RGX stream");
    }

    Ok(RecipientEnvelope {
        recipients,
        password,
        payload_offset: expected_header_len as u64,
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

pub fn unlock_password(path: &Path, password: &str) -> Result<(Zeroizing<ArchiveKey>, UnlockMethod)> {
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
    with_inner_payload(path, |inner| {
        let secret = archive_key_password(archive_key);
        match selected {
            Some(selected) => private::extract_selected_private(inner, output, selected, secret.as_str()),
            None => private::extract_private(inner, output, secret.as_str()),
        }
    })
}

pub fn verify(path: &Path, archive_key: &ArchiveKey) -> Result<ArchiveInfo> {
    with_inner_payload(path, |inner| {
        let secret = archive_key_password(archive_key);
        private::verify_private(inner, secret.as_str())
    })
}

pub fn info(path: &Path, archive_key: &ArchiveKey) -> Result<ArchiveInfo> {
    with_inner_payload(path, |inner| {
        let secret = archive_key_password(archive_key);
        private::info_private(inner, secret.as_str())
    })
}

pub fn list(path: &Path, archive_key: &ArchiveKey) -> Result<Vec<ArchiveEntry>> {
    with_inner_payload(path, |inner| {
        let secret = archive_key_password(archive_key);
        private::list_private(inner, secret.as_str())
    })
}

pub fn find(path: &Path, query: &str, archive_key: &ArchiveKey) -> Result<Vec<ArchiveEntry>> {
    with_inner_payload(path, |inner| {
        let secret = archive_key_password(archive_key);
        private::find_private(inner, query, secret.as_str())
    })
}

pub fn read_entry(path: &Path, entry: &str, archive_key: &ArchiveKey) -> Result<Vec<u8>> {
    with_inner_payload(path, |inner| {
        let secret = archive_key_password(archive_key);
        private::read_entry_private(inner, entry, secret.as_str())
    })
}

fn write_envelope<W: Write>(
    writer: &mut W,
    recipients: &[RecipientSlot],
    password: Option<&PasswordSlot>,
) -> Result<()> {
    let flags = if password.is_some() {
        FLAG_PASSWORD_FALLBACK
    } else {
        0
    };
    let header_len = PREFIX_SIZE
        .checked_add(recipients.len() * RECIPIENT_SLOT_SIZE)
        .and_then(|value| value.checked_add(if password.is_some() { PASSWORD_SLOT_SIZE } else { 0 }))
        .ok_or_else(|| anyhow!("recipient RGX header length overflow"))?;
    let header_len = u32::try_from(header_len).context("recipient RGX header is too large")?;
    let recipient_count = u16::try_from(recipients.len()).context("too many RGX recipients")?;

    let mut prefix = [0u8; PREFIX_SIZE];
    prefix[0..4].copy_from_slice(&RECIPIENT_MAGIC);
    prefix[4..6].copy_from_slice(&VERSION.to_le_bytes());
    prefix[6..8].copy_from_slice(&flags.to_le_bytes());
    prefix[8..10].copy_from_slice(&recipient_count.to_le_bytes());
    prefix[12..16].copy_from_slice(&header_len.to_le_bytes());
    writer.write_all(&prefix)?;

    for slot in recipients {
        writer.write_all(&slot.key_id)?;
        writer.write_all(&slot.ephemeral_public)?;
        writer.write_all(&slot.nonce)?;
        writer.write_all(&slot.wrapped_key)?;
    }
    if let Some(slot) = password {
        writer.write_all(&slot.memory_kib.to_le_bytes())?;
        writer.write_all(&slot.iterations.to_le_bytes())?;
        writer.write_all(&slot.lanes.to_le_bytes())?;
        writer.write_all(&slot.salt)?;
        writer.write_all(&slot.nonce)?;
        writer.write_all(&slot.wrapped_key)?;
    }
    Ok(())
}

fn read_recipient_slot<R: Read>(reader: &mut R) -> Result<RecipientSlot> {
    let mut key_id = [0u8; 16];
    let mut ephemeral_public = [0u8; 32];
    let mut nonce = [0u8; 24];
    let mut wrapped_key = [0u8; 48];
    reader.read_exact(&mut key_id)?;
    reader.read_exact(&mut ephemeral_public)?;
    reader.read_exact(&mut nonce)?;
    reader.read_exact(&mut wrapped_key)?;
    Ok(RecipientSlot {
        key_id,
        ephemeral_public,
        nonce,
        wrapped_key,
    })
}

fn read_password_slot<R: Read>(reader: &mut R) -> Result<PasswordSlot> {
    let memory_kib = read_u32(reader)?;
    let iterations = read_u32(reader)?;
    let lanes = read_u32(reader)?;
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 24];
    let mut wrapped_key = [0u8; 48];
    reader.read_exact(&mut salt)?;
    reader.read_exact(&mut nonce)?;
    reader.read_exact(&mut wrapped_key)?;
    Ok(PasswordSlot {
        memory_kib,
        iterations,
        lanes,
        salt,
        nonce,
        wrapped_key,
    })
}

fn read_u32<R: Read>(reader: &mut R) -> Result<u32> {
    let mut bytes = [0u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn with_inner_payload<T>(path: &Path, action: impl FnOnce(&Path) -> Result<T>) -> Result<T> {
    let envelope = read_envelope(path)?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = NamedTempFile::new_in(parent).context("failed to create RGX recipient payload view")?;
    let mut source = BufReader::new(File::open(path)?);
    source.seek(SeekFrom::Start(envelope.payload_offset))?;
    std::io::copy(&mut source, temp.as_file_mut())?;
    temp.as_file_mut().flush()?;
    action(temp.path())
}

fn archive_key_password(archive_key: &ArchiveKey) -> Zeroizing<String> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(64 + 18);
    value.push_str("RGX-ARCHIVE-KEY-1:");
    for &byte in archive_key {
        value.push(HEX[(byte >> 4) as usize] as char);
        value.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Zeroizing::new(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn envelope_roundtrip_preserves_slots() {
        let recipient_key = RgxPrivateKey::generate();
        let archive_key = recipient::random_archive_key();
        let recipient_slot = recipient::wrap_archive_key_for_recipient(
            &archive_key,
            &recipient_key.public_key(),
        )
        .unwrap();
        let password_slot =
            recipient::wrap_archive_key_with_password(&archive_key, "fallback password").unwrap();
        let mut bytes = Vec::new();
        write_envelope(&mut bytes, &[recipient_slot.clone()], Some(&password_slot)).unwrap();
        assert_eq!(&bytes[..4], b"RGXR");
        assert_eq!(bytes.len(), PREFIX_SIZE + RECIPIENT_SLOT_SIZE + PASSWORD_SLOT_SIZE);
    }

    #[test]
    fn recipient_archive_unlocks_with_identity_and_password() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("hello.txt"), b"recipient archive test").unwrap();

        let private_path = temp.path().join("id_rgx");
        let identity = RgxPrivateKey::generate();
        let public_path = recipient::save_keypair(&private_path, &identity).unwrap();
        let public = recipient::load_public_key(&public_path).unwrap();
        let archive_path = temp.path().join("recipient.rgx");
        pack_recipient(
            &source,
            &archive_path,
            3,
            &[public],
            Some("fallback password"),
        )
        .unwrap();

        let (key, _) = try_unlock_identity(&archive_path, Some(&private_path))
            .unwrap()
            .unwrap();
        verify(&archive_path, key.as_ref()).unwrap();

        let (fallback_key, _) = unlock_password(&archive_path, "fallback password").unwrap();
        verify(&archive_path, fallback_key.as_ref()).unwrap();
        assert!(unlock_password(&archive_path, "wrong password").is_err());
    }
}
