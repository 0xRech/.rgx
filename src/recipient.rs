use anyhow::{anyhow, bail, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use rand_core::{OsRng, RngCore};
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

pub const PRIVATE_KEY_HEADER: &str = "RGX-PRIVATE-KEY-1";
pub const PUBLIC_KEY_HEADER: &str = "RGX-PUBLIC-KEY-1";
const RECIPIENT_WRAP_CONTEXT: &str = "rgx recipient archive-key wrap v1";
const RECIPIENT_WRAP_AAD: &[u8] = b"RGX-RECIPIENT-WRAP-1";
const PASSWORD_WRAP_AAD: &[u8] = b"RGX-PASSWORD-WRAP-1";
const WRAPPED_KEY_LEN: usize = 48;
const ARGON2_MEMORY_KIB: u32 = 64 * 1024;
const ARGON2_ITERATIONS: u32 = 3;
const ARGON2_LANES: u32 = 1;

pub type ArchiveKey = [u8; 32];
pub type KeyId = [u8; 16];

pub struct RgxPrivateKey {
    secret: StaticSecret,
}

#[derive(Clone, Copy)]
pub struct RgxPublicKey {
    public: PublicKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipientSlot {
    pub key_id: KeyId,
    pub ephemeral_public: [u8; 32],
    pub nonce: [u8; 24],
    pub wrapped_key: [u8; WRAPPED_KEY_LEN],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordSlot {
    pub memory_kib: u32,
    pub iterations: u32,
    pub lanes: u32,
    pub salt: [u8; 16],
    pub nonce: [u8; 24],
    pub wrapped_key: [u8; WRAPPED_KEY_LEN],
}

impl RgxPrivateKey {
    pub fn generate() -> Self {
        Self {
            secret: StaticSecret::random_from_rng(OsRng),
        }
    }

    pub fn public_key(&self) -> RgxPublicKey {
        RgxPublicKey {
            public: PublicKey::from(&self.secret),
        }
    }

    pub fn key_id(&self) -> KeyId {
        self.public_key().key_id()
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self {
            secret: StaticSecret::from(bytes),
        }
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.secret.to_bytes()
    }
}

impl RgxPublicKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self {
            public: PublicKey::from(bytes),
        }
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.public.to_bytes()
    }

    pub fn key_id(&self) -> KeyId {
        let digest = blake3::hash(self.public.as_bytes());
        let mut id = [0u8; 16];
        id.copy_from_slice(&digest.as_bytes()[..16]);
        id
    }
}

pub fn random_archive_key() -> ArchiveKey {
    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    key
}

pub fn wrap_archive_key_for_recipient(
    archive_key: &ArchiveKey,
    recipient: &RgxPublicKey,
) -> Result<RecipientSlot> {
    let ephemeral_secret = StaticSecret::random_from_rng(OsRng);
    let ephemeral_public = PublicKey::from(&ephemeral_secret);
    let shared = ephemeral_secret.diffie_hellman(&recipient.public);
    let key_id = recipient.key_id();
    let wrap_key = derive_recipient_wrap_key(
        shared.as_bytes(),
        ephemeral_public.as_bytes(),
        recipient.public.as_bytes(),
        &key_id,
    );

    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let cipher = XChaCha20Poly1305::new_from_slice(&wrap_key)
        .map_err(|_| anyhow!("failed to initialize recipient key wrapper"))?;
    let aad = recipient_aad(&key_id, ephemeral_public.as_bytes());
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: archive_key,
                aad: &aad,
            },
        )
        .map_err(|_| anyhow!("failed to wrap RGX archive key for recipient"))?;
    let wrapped_key: [u8; WRAPPED_KEY_LEN] = ciphertext
        .try_into()
        .map_err(|_| anyhow!("unexpected wrapped RGX archive-key length"))?;

    Ok(RecipientSlot {
        key_id,
        ephemeral_public: ephemeral_public.to_bytes(),
        nonce,
        wrapped_key,
    })
}

pub fn unwrap_archive_key_for_recipient(
    slot: &RecipientSlot,
    private_key: &RgxPrivateKey,
) -> Result<Zeroizing<ArchiveKey>> {
    let public = private_key.public_key();
    if public.key_id() != slot.key_id {
        bail!("RGX private key does not match this recipient slot");
    }

    let ephemeral_public = PublicKey::from(slot.ephemeral_public);
    let shared = private_key.secret.diffie_hellman(&ephemeral_public);
    let wrap_key = derive_recipient_wrap_key(
        shared.as_bytes(),
        &slot.ephemeral_public,
        public.public.as_bytes(),
        &slot.key_id,
    );
    let cipher = XChaCha20Poly1305::new_from_slice(&wrap_key)
        .map_err(|_| anyhow!("failed to initialize recipient key wrapper"))?;
    let aad = recipient_aad(&slot.key_id, &slot.ephemeral_public);
    let plaintext = cipher
        .decrypt(
            XNonce::from_slice(&slot.nonce),
            Payload {
                msg: &slot.wrapped_key,
                aad: &aad,
            },
        )
        .map_err(|_| anyhow!("RGX recipient archive-key authentication failed"))?;
    let key: ArchiveKey = plaintext
        .try_into()
        .map_err(|_| anyhow!("invalid unwrapped RGX archive-key length"))?;
    Ok(Zeroizing::new(key))
}

pub fn wrap_archive_key_with_password(
    archive_key: &ArchiveKey,
    password: &str,
) -> Result<PasswordSlot> {
    if password.is_empty() {
        bail!("RGX password fallback must not be empty");
    }
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);
    let wrap_key = derive_password_wrap_key(
        password,
        &salt,
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_LANES,
    )?;
    let cipher = XChaCha20Poly1305::new_from_slice(wrap_key.as_ref())
        .map_err(|_| anyhow!("failed to initialize password key wrapper"))?;
    let aad = password_aad(&salt, ARGON2_MEMORY_KIB, ARGON2_ITERATIONS, ARGON2_LANES);
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: archive_key,
                aad: &aad,
            },
        )
        .map_err(|_| anyhow!("failed to wrap RGX archive key with password"))?;
    let wrapped_key: [u8; WRAPPED_KEY_LEN] = ciphertext
        .try_into()
        .map_err(|_| anyhow!("unexpected password-wrapped RGX archive-key length"))?;

    Ok(PasswordSlot {
        memory_kib: ARGON2_MEMORY_KIB,
        iterations: ARGON2_ITERATIONS,
        lanes: ARGON2_LANES,
        salt,
        nonce,
        wrapped_key,
    })
}

pub fn unwrap_archive_key_with_password(
    slot: &PasswordSlot,
    password: &str,
) -> Result<Zeroizing<ArchiveKey>> {
    validate_password_parameters(slot.memory_kib, slot.iterations, slot.lanes)?;
    let wrap_key = derive_password_wrap_key(
        password,
        &slot.salt,
        slot.memory_kib,
        slot.iterations,
        slot.lanes,
    )?;
    let cipher = XChaCha20Poly1305::new_from_slice(wrap_key.as_ref())
        .map_err(|_| anyhow!("failed to initialize password key wrapper"))?;
    let aad = password_aad(&slot.salt, slot.memory_kib, slot.iterations, slot.lanes);
    let plaintext = cipher
        .decrypt(
            XNonce::from_slice(&slot.nonce),
            Payload {
                msg: &slot.wrapped_key,
                aad: &aad,
            },
        )
        .map_err(|_| anyhow!("wrong password or damaged RGX password recipient slot"))?;
    let key: ArchiveKey = plaintext
        .try_into()
        .map_err(|_| anyhow!("invalid password-unwrapped RGX archive-key length"))?;
    Ok(Zeroizing::new(key))
}

pub fn save_keypair(private_path: &Path, private_key: &RgxPrivateKey) -> Result<PathBuf> {
    if private_path.exists() {
        bail!("RGX private key already exists: {}", private_path.display());
    }
    let public_path = public_path_for(private_path);
    if public_path.exists() {
        bail!("RGX public key already exists: {}", public_path.display());
    }
    if let Some(parent) = private_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let private_text = format!(
        "{PRIVATE_KEY_HEADER}\n{}\n",
        hex_encode(&private_key.to_bytes())
    );
    let public_key = private_key.public_key();
    let public_text = format!(
        "{PUBLIC_KEY_HEADER}\n{}\nKey-ID: {}\n",
        hex_encode(&public_key.to_bytes()),
        format_key_id(&public_key.key_id())
    );

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut private_file = options
        .open(private_path)
        .with_context(|| format!("failed to create {}", private_path.display()))?;
    if let Err(error) = private_file.write_all(private_text.as_bytes()) {
        let _ = fs::remove_file(private_path);
        return Err(error).context("failed to write RGX private key");
    }
    private_file.flush()?;

    if let Err(error) = fs::write(&public_path, public_text) {
        let _ = fs::remove_file(private_path);
        return Err(error).context("failed to write RGX public key");
    }
    Ok(public_path)
}

pub fn generate_and_save_keypair(private_path: &Path) -> Result<(KeyId, PathBuf)> {
    let private_key = RgxPrivateKey::generate();
    let key_id = private_key.key_id();
    let public_path = save_keypair(private_path, &private_key)?;
    Ok((key_id, public_path))
}

pub fn load_private_key(path: &Path) -> Result<RgxPrivateKey> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read RGX private key {}", path.display()))?;
    let bytes = parse_key_file(&text, PRIVATE_KEY_HEADER)?;
    Ok(RgxPrivateKey::from_bytes(bytes))
}

pub fn load_public_key(path: &Path) -> Result<RgxPublicKey> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read RGX public key {}", path.display()))?;
    let bytes = parse_key_file(&text, PUBLIC_KEY_HEADER)?;
    Ok(RgxPublicKey::from_bytes(bytes))
}

pub fn find_matching_private_key(
    slots: &[RecipientSlot],
    explicit: Option<&Path>,
) -> Result<Option<(PathBuf, RgxPrivateKey)>> {
    if let Some(path) = explicit {
        let key = load_private_key(path)?;
        if slots.iter().any(|slot| slot.key_id == key.key_id()) {
            return Ok(Some((path.to_path_buf(), key)));
        }
        bail!(
            "explicit RGX identity {} does not match any archive recipient",
            path.display()
        );
    }

    for path in default_private_key_candidates() {
        if !path.is_file() {
            continue;
        }
        if let Ok(key) = load_private_key(&path) {
            if slots.iter().any(|slot| slot.key_id == key.key_id()) {
                return Ok(Some((path, key)));
            }
        }
    }
    Ok(None)
}

pub fn default_private_key_path() -> Result<PathBuf> {
    if let Some(home) = home_dir() {
        return Ok(home.join(".ssh").join("id_rgx"));
    }
    bail!("could not determine user home directory for the default RGX key path")
}

pub fn default_private_key_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(home) = home_dir() {
        candidates.push(home.join(".ssh").join("id_rgx"));
        candidates.push(home.join(".config").join("rgx").join("keys").join("id_rgx"));
    }
    if cfg!(windows) {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            candidates.push(
                PathBuf::from(appdata)
                    .join("RGX")
                    .join("keys")
                    .join("id_rgx"),
            );
        }
    }
    candidates.dedup();
    candidates
}

pub fn public_path_for(private_path: &Path) -> PathBuf {
    let mut name: OsString = private_path
        .file_name()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| OsString::from("id_rgx"));
    name.push(".pub");
    private_path.with_file_name(name)
}

pub fn format_key_id(key_id: &KeyId) -> String {
    hex_encode(key_id)
}

fn derive_recipient_wrap_key(
    shared_secret: &[u8; 32],
    ephemeral_public: &[u8; 32],
    recipient_public: &[u8; 32],
    key_id: &KeyId,
) -> [u8; 32] {
    let mut material = Vec::with_capacity(32 + 32 + 32 + 16);
    material.extend_from_slice(shared_secret);
    material.extend_from_slice(ephemeral_public);
    material.extend_from_slice(recipient_public);
    material.extend_from_slice(key_id);
    blake3::derive_key(RECIPIENT_WRAP_CONTEXT, &material)
}

fn recipient_aad(key_id: &KeyId, ephemeral_public: &[u8; 32]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(RECIPIENT_WRAP_AAD.len() + 16 + 32);
    aad.extend_from_slice(RECIPIENT_WRAP_AAD);
    aad.extend_from_slice(key_id);
    aad.extend_from_slice(ephemeral_public);
    aad
}

fn derive_password_wrap_key(
    password: &str,
    salt: &[u8; 16],
    memory_kib: u32,
    iterations: u32,
    lanes: u32,
) -> Result<Zeroizing<[u8; 32]>> {
    if password.is_empty() {
        bail!("RGX password fallback must not be empty");
    }
    validate_password_parameters(memory_kib, iterations, lanes)?;
    let params = Params::new(memory_kib, iterations, lanes, Some(32))
        .map_err(|error| anyhow!("invalid Argon2 parameters: {error}"))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|error| anyhow!("Argon2id password-fallback derivation failed: {error}"))?;
    Ok(key)
}

fn password_aad(salt: &[u8; 16], memory_kib: u32, iterations: u32, lanes: u32) -> Vec<u8> {
    let mut aad = Vec::with_capacity(PASSWORD_WRAP_AAD.len() + 28);
    aad.extend_from_slice(PASSWORD_WRAP_AAD);
    aad.extend_from_slice(&memory_kib.to_le_bytes());
    aad.extend_from_slice(&iterations.to_le_bytes());
    aad.extend_from_slice(&lanes.to_le_bytes());
    aad.extend_from_slice(salt);
    aad
}

fn validate_password_parameters(memory_kib: u32, iterations: u32, lanes: u32) -> Result<()> {
    if !(8 * 1024..=1024 * 1024).contains(&memory_kib) {
        bail!("RGX password-fallback Argon2 memory parameter is outside the accepted range");
    }
    if !(1..=10).contains(&iterations) {
        bail!("RGX password-fallback Argon2 iteration parameter is outside the accepted range");
    }
    if !(1..=16).contains(&lanes) {
        bail!("RGX password-fallback Argon2 lane parameter is outside the accepted range");
    }
    Ok(())
}

fn parse_key_file(text: &str, expected_header: &str) -> Result<[u8; 32]> {
    let mut lines = text.lines();
    let header = lines.next().unwrap_or_default().trim();
    if header != expected_header {
        bail!("not a supported RGX key file ({expected_header} expected)");
    }
    let encoded = lines
        .next()
        .ok_or_else(|| anyhow!("RGX key file is missing key material"))?
        .trim();
    hex_decode_32(encoded)
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

fn hex_decode_32(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64 {
        bail!("RGX key material must contain exactly 32 bytes");
    }
    let mut output = [0u8; 32];
    let bytes = value.as_bytes();
    for index in 0..32 {
        let high = decode_nibble(bytes[index * 2])?;
        let low = decode_nibble(bytes[index * 2 + 1])?;
        output[index] = (high << 4) | low;
    }
    Ok(output)
}

fn decode_nibble(value: u8) -> Result<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => bail!("RGX key contains non-hexadecimal data"),
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn recipient_wrap_roundtrip_and_wrong_key_rejection() {
        let recipient = RgxPrivateKey::generate();
        let wrong = RgxPrivateKey::generate();
        let archive_key = random_archive_key();
        let slot = wrap_archive_key_for_recipient(&archive_key, &recipient.public_key()).unwrap();

        let restored = unwrap_archive_key_for_recipient(&slot, &recipient).unwrap();
        assert_eq!(restored.as_ref(), &archive_key);
        assert!(unwrap_archive_key_for_recipient(&slot, &wrong).is_err());
    }

    #[test]
    fn password_fallback_roundtrip_and_wrong_password_rejection() {
        let archive_key = random_archive_key();
        let slot =
            wrap_archive_key_with_password(&archive_key, "recipient fallback password").unwrap();
        let restored =
            unwrap_archive_key_with_password(&slot, "recipient fallback password").unwrap();
        assert_eq!(restored.as_ref(), &archive_key);
        assert!(unwrap_archive_key_with_password(&slot, "wrong password").is_err());
    }

    #[test]
    fn key_files_roundtrip() {
        let temp = tempdir().unwrap();
        let private_path = temp.path().join(".ssh/id_rgx");
        let key = RgxPrivateKey::generate();
        let public_path = save_keypair(&private_path, &key).unwrap();

        let loaded_private = load_private_key(&private_path).unwrap();
        let loaded_public = load_public_key(&public_path).unwrap();
        assert_eq!(loaded_private.key_id(), key.key_id());
        assert_eq!(loaded_public.key_id(), key.key_id());
        assert!(save_keypair(&private_path, &key).is_err());
    }
}
