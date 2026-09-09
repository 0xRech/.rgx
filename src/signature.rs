use crate::recipient::{self, KeyId, RgxPrivateKey};
use anyhow::{anyhow, bail, Context, Result};
use ed25519_dalek::{Signature, Signer, Verifier};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const SIGNATURE_HEADER: &str = "RGX-SIGNATURE-1";
const SIGNATURE_CONTEXT: &[u8] = b"RGX-ARCHIVE-SIGNATURE-1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureInfo {
    pub key_id: KeyId,
    pub archive_len: u64,
    pub archive_hash: [u8; 32],
}

pub fn default_signature_path(archive: &Path) -> PathBuf {
    let mut output = archive.as_os_str().to_os_string();
    output.push(".sig");
    PathBuf::from(output)
}

pub fn sign_archive(
    archive: &Path,
    private_key: &RgxPrivateKey,
    output: &Path,
) -> Result<SignatureInfo> {
    if output.exists() {
        bail!("signature output already exists: {}", output.display());
    }
    let (archive_len, archive_hash) = hash_archive(archive)?;
    let key_id = private_key.key_id();
    let message = signature_message(archive_len, &archive_hash);
    let signature = private_key.signing_key().sign(&message);
    let text = format!(
        "{SIGNATURE_HEADER}\nAlgorithm: Ed25519\nKey-ID: {}\nArchive-Length: {}\nArchive-BLAKE3: {}\nSignature: {}\n",
        recipient::format_key_id(&key_id),
        archive_len,
        hex_encode(&archive_hash),
        hex_encode(&signature.to_bytes())
    );
    fs::write(output, text)
        .with_context(|| format!("failed to write RGX signature {}", output.display()))?;
    Ok(SignatureInfo {
        key_id,
        archive_len,
        archive_hash,
    })
}

pub fn verify_archive_signature(
    archive: &Path,
    signature_path: &Path,
    public_key_path: &Path,
) -> Result<SignatureInfo> {
    let text = fs::read_to_string(signature_path)
        .with_context(|| format!("failed to read RGX signature {}", signature_path.display()))?;
    let record = parse_signature(&text)?;
    let recipient_public = recipient::load_public_key(public_key_path)?;
    if recipient_public.key_id() != record.key_id {
        bail!("RGX signature Key-ID does not match the supplied public key");
    }
    let signing_public = recipient::load_signing_public_key(public_key_path)?;
    let (archive_len, archive_hash) = hash_archive(archive)?;
    if archive_len != record.archive_len || archive_hash != record.archive_hash {
        bail!("RGX archive bytes do not match the signed BLAKE3 digest");
    }
    let message = signature_message(archive_len, &archive_hash);
    let signature = Signature::from_bytes(&record.signature);
    signing_public
        .verify(&message, &signature)
        .map_err(|_| anyhow!("RGX Ed25519 signature verification failed"))?;
    Ok(SignatureInfo {
        key_id: record.key_id,
        archive_len,
        archive_hash,
    })
}

pub fn format_hash(hash: &[u8; 32]) -> String {
    hex_encode(hash)
}

struct SignatureRecord {
    key_id: KeyId,
    archive_len: u64,
    archive_hash: [u8; 32],
    signature: [u8; 64],
}

fn parse_signature(text: &str) -> Result<SignatureRecord> {
    let header = text.lines().next().unwrap_or_default().trim();
    if header != SIGNATURE_HEADER {
        bail!("not a supported RGX signature file");
    }
    if field(text, "Algorithm:")? != "Ed25519" {
        bail!("unsupported RGX signature algorithm");
    }
    let key_id = hex_decode::<16>(field(text, "Key-ID:")?)?;
    let archive_len = field(text, "Archive-Length:")?
        .parse::<u64>()
        .context("invalid RGX signature archive length")?;
    let archive_hash = hex_decode::<32>(field(text, "Archive-BLAKE3:")?)?;
    let signature = hex_decode::<64>(field(text, "Signature:")?)?;
    Ok(SignatureRecord {
        key_id,
        archive_len,
        archive_hash,
        signature,
    })
}

fn signature_message(archive_len: u64, archive_hash: &[u8; 32]) -> Vec<u8> {
    let mut message = Vec::with_capacity(SIGNATURE_CONTEXT.len() + 8 + 32);
    message.extend_from_slice(SIGNATURE_CONTEXT);
    message.extend_from_slice(&archive_len.to_le_bytes());
    message.extend_from_slice(archive_hash);
    message
}

fn hash_archive(path: &Path) -> Result<(u64, [u8; 32])> {
    let mut file =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    let mut total = 0u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| anyhow!("RGX archive length overflow while signing"))?;
    }
    Ok((total, *hasher.finalize().as_bytes()))
}

fn field<'a>(text: &'a str, label: &str) -> Result<&'a str> {
    text.lines()
        .find_map(|line| line.strip_prefix(label).map(str::trim))
        .ok_or_else(|| anyhow!("RGX signature file is missing {label}"))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn hex_decode<const N: usize>(value: &str) -> Result<[u8; N]> {
    if value.len() != N * 2 {
        bail!("RGX signature field has an unexpected length");
    }
    let bytes = value.as_bytes();
    let mut output = [0u8; N];
    for index in 0..N {
        output[index] =
            (decode_nibble(bytes[index * 2])? << 4) | decode_nibble(bytes[index * 2 + 1])?;
    }
    Ok(output)
}

fn decode_nibble(value: u8) -> Result<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => bail!("RGX signature contains non-hexadecimal data"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipient;
    use tempfile::tempdir;

    #[test]
    fn detached_signature_roundtrip_and_tamper_rejection() {
        let temp = tempdir().unwrap();
        let archive = temp.path().join("sample.rgx");
        let private_path = temp.path().join("id_rgx");
        let signature_path = temp.path().join("sample.rgx.sig");
        fs::write(&archive, b"synthetic rgx archive bytes").unwrap();

        let private_key = recipient::RgxPrivateKey::generate();
        let public_path = recipient::save_keypair(&private_path, &private_key).unwrap();
        sign_archive(&archive, &private_key, &signature_path).unwrap();
        verify_archive_signature(&archive, &signature_path, &public_path).unwrap();

        fs::write(&archive, b"synthetic rgx archive bytes changed").unwrap();
        assert!(verify_archive_signature(&archive, &signature_path, &public_path).is_err());
    }
}
