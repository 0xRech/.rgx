use anyhow::{anyhow, bail, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand_core::{OsRng, RngCore};
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

pub const PRIVATE_KEY_HEADER: &str = "RGX-PRIVATE-KEY-2";
pub const PUBLIC_KEY_HEADER: &str = "RGX-PUBLIC-KEY-2";
pub const PROTECTED_PRIVATE_KEY_HEADER: &str = "RGX-PRIVATE-KEY-2-ENCRYPTED";
const LEGACY_PRIVATE_KEY_HEADER: &str = "RGX-PRIVATE-KEY-1";
const LEGACY_PUBLIC_KEY_HEADER: &str = "RGX-PUBLIC-KEY-1";
const RECIPIENT_WRAP_CONTEXT: &str = "rgx recipient archive-key wrap v1";
const RECIPIENT_WRAP_AAD: &[u8] = b"RGX-RECIPIENT-WRAP-1";
const PASSWORD_WRAP_AAD: &[u8] = b"RGX-PASSWORD-WRAP-1";
const PRIVATE_KEY_FILE_AAD: &[u8] = b"RGX-PRIVATE-KEY-FILE-2";
const SIGNING_SEED_CONTEXT: &str = "rgx ed25519 signing seed v1";
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

    pub fn signing_key(&self) -> SigningKey {
        let seed = blake3::derive_key(SIGNING_SEED_CONTEXT, &self.to_bytes());
        SigningKey::from_bytes(&seed)
    }

    pub fn signing_public_key(&self) -> VerifyingKey {
        self.signing_key().verifying_key()
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
    let wrap_key = derive_argon2_key(
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
    let wrap_key = derive_argon2_key(
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
    let private_text = format!(
        "{PRIVATE_KEY_HEADER}\nX25519-Secret: {}\nKey-ID: {}\n",
        hex_encode(&private_key.to_bytes()),
        format_key_id(&private_key.key_id())
    );
    save_keypair_text(private_path, private_key, private_text)
}

pub fn save_keypair_protected(
    private_path: &Path,
    private_key: &RgxPrivateKey,
    password: &str,
) -> Result<PathBuf> {
    if password.is_empty() {
        bail!("RGX key passphrase must not be empty");
    }
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);

    let key_id = private_key.key_id();
    let public = private_key.public_key().to_bytes();
    let wrap_key = derive_argon2_key(
        password,
        &salt,
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_LANES,
    )?;
    let aad = private_key_file_aad(
        &key_id,
        &public,
        &salt,
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_LANES,
    );
    let cipher = XChaCha20Poly1305::new_from_slice(wrap_key.as_ref())
        .map_err(|_| anyhow!("failed to initialize RGX private-key protector"))?;
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &private_key.to_bytes(),
                aad: &aad,
            },
        )
        .map_err(|_| anyhow!("failed to protect RGX private key"))?;
    let ciphertext: [u8; WRAPPED_KEY_LEN] = ciphertext
        .try_into()
        .map_err(|_| anyhow!("unexpected protected RGX private-key length"))?;

    let private_text = format!(
        "{PROTECTED_PRIVATE_KEY_HEADER}\nKey-ID: {}\nX25519-Public: {}\nArgon2-Memory-KiB: {}\nArgon2-Iterations: {}\nArgon2-Lanes: {}\nSalt: {}\nNonce: {}\nCiphertext: {}\n",
        format_key_id(&key_id),
        hex_encode(&public),
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_LANES,
        hex_encode(&salt),
        hex_encode(&nonce),
        hex_encode(&ciphertext)
    );
    save_keypair_text(private_path, private_key, private_text)
}

fn save_keypair_text(
    private_path: &Path,
    private_key: &RgxPrivateKey,
    private_text: String,
) -> Result<PathBuf> {
    if private_path.exists() {
        bail!("RGX private key already exists: {}", private_path.display());
    }
    let public_path = public_path_for(private_path);
    if public_path.exists() {
        bail!("RGX public key already exists: {}", public_path.display());
    }
    if let Some(parent) = private_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }

    let public_key = private_key.public_key();
    let signing_public = private_key.signing_public_key();
    let public_text = format!(
        "{PUBLIC_KEY_HEADER}\nX25519-Public: {}\nEd25519-Public: {}\nKey-ID: {}\n",
        hex_encode(&public_key.to_bytes()),
        hex_encode(&signing_public.to_bytes()),
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

pub fn generate_and_save_keypair_protected(
    private_path: &Path,
    password: &str,
) -> Result<(KeyId, PathBuf)> {
    let private_key = RgxPrivateKey::generate();
    let key_id = private_key.key_id();
    let public_path = save_keypair_protected(private_path, &private_key, password)?;
    Ok((key_id, public_path))
}

pub fn load_private_key(path: &Path) -> Result<RgxPrivateKey> {
    let password = std::env::var("RGX_KEY_PASSWORD").ok();
    load_private_key_with_password(path, password.as_deref())
}

pub fn load_private_key_with_password(
    path: &Path,
    password: Option<&str>,
) -> Result<RgxPrivateKey> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read RGX private key {}", path.display()))?;
    let header = text.lines().next().unwrap_or_default().trim();
    match header {
        LEGACY_PRIVATE_KEY_HEADER => {
            let bytes = parse_legacy_key_file(&text, LEGACY_PRIVATE_KEY_HEADER)?;
            Ok(RgxPrivateKey::from_bytes(bytes))
        }
        PRIVATE_KEY_HEADER => {
            let bytes = hex_decode::<32>(field(&text, "X25519-Secret:")?)?;
            let key = RgxPrivateKey::from_bytes(bytes);
            if let Ok(encoded_id) = field(&text, "Key-ID:") {
                let key_id = hex_decode::<16>(encoded_id)?;
                if key.key_id() != key_id {
                    bail!("RGX private-key metadata does not match key material");
                }
            }
            Ok(key)
        }
        PROTECTED_PRIVATE_KEY_HEADER => load_protected_private_key(&text, password),
        _ => bail!("not a supported RGX private key file"),
    }
}

pub fn private_key_is_protected(path: &Path) -> Result<bool> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read RGX private key {}", path.display()))?;
    Ok(text.lines().next().unwrap_or_default().trim() == PROTECTED_PRIVATE_KEY_HEADER)
}

fn load_protected_private_key(text: &str, password: Option<&str>) -> Result<RgxPrivateKey> {
    let password = password.ok_or_else(|| {
        anyhow!(
            "RGX private key is passphrase-protected; set RGX_KEY_PASSWORD or provide the key passphrase explicitly"
        )
    })?;
    if password.is_empty() {
        bail!("RGX key passphrase must not be empty");
    }

    let key_id = hex_decode::<16>(field(text, "Key-ID:")?)?;
    let public = hex_decode::<32>(field(text, "X25519-Public:")?)?;
    let memory_kib = parse_u32_field(text, "Argon2-Memory-KiB:")?;
    let iterations = parse_u32_field(text, "Argon2-Iterations:")?;
    let lanes = parse_u32_field(text, "Argon2-Lanes:")?;
    let salt = hex_decode::<16>(field(text, "Salt:")?)?;
    let nonce = hex_decode::<24>(field(text, "Nonce:")?)?;
    let ciphertext = hex_decode::<WRAPPED_KEY_LEN>(field(text, "Ciphertext:")?)?;

    validate_password_parameters(memory_kib, iterations, lanes)?;
    let wrap_key = derive_argon2_key(password, &salt, memory_kib, iterations, lanes)?;
    let aad = private_key_file_aad(&key_id, &public, &salt, memory_kib, iterations, lanes);
    let cipher = XChaCha20Poly1305::new_from_slice(wrap_key.as_ref())
        .map_err(|_| anyhow!("failed to initialize RGX private-key protector"))?;
    let plaintext = cipher
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| anyhow!("wrong RGX key passphrase or damaged protected private key"))?;
    let secret: [u8; 32] = plaintext
        .try_into()
        .map_err(|_| anyhow!("invalid protected RGX private-key length"))?;
    let key = RgxPrivateKey::from_bytes(secret);
    if key.public_key().to_bytes() != public || key.key_id() != key_id {
        bail!("protected RGX private-key metadata does not match decrypted key material");
    }
    Ok(key)
}

pub fn load_public_key(path: &Path) -> Result<RgxPublicKey> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read RGX public key {}", path.display()))?;
    let header = text.lines().next().unwrap_or_default().trim();
    let bytes = match header {
        LEGACY_PUBLIC_KEY_HEADER => parse_legacy_key_file(&text, LEGACY_PUBLIC_KEY_HEADER)?,
        PUBLIC_KEY_HEADER => hex_decode::<32>(field(&text, "X25519-Public:")?)?,
        _ => bail!("not a supported RGX public key file"),
    };
    Ok(RgxPublicKey::from_bytes(bytes))
}

pub fn load_signing_public_key(path: &Path) -> Result<VerifyingKey> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read RGX public key {}", path.display()))?;
    let header = text.lines().next().unwrap_or_default().trim();
    if header != PUBLIC_KEY_HEADER {
        bail!("this RGX public key does not include an Ed25519 signing key; use a v0.5 keypair");
    }
    let bytes = hex_decode::<32>(field(&text, "Ed25519-Public:")?)?;
    VerifyingKey::from_bytes(&bytes)
        .map_err(|_| anyhow!("invalid Ed25519 public key in RGX key file"))
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
        match load_private_key(&path) {
            Ok(key) if slots.iter().any(|slot| slot.key_id == key.key_id()) => {
                return Ok(Some((path, key)));
            }
            Ok(_) => {}
            Err(error) => {
                if private_key_is_protected(&path).unwrap_or(false) {
                    if let Ok(key_id) = protected_key_id(&path) {
                        if slots.iter().any(|slot| slot.key_id == key_id) {
                            return Err(error).with_context(|| {
                                format!(
                                    "matching protected RGX identity found at {}; set RGX_KEY_PASSWORD",
                                    path.display()
                                )
                            });
                        }
                    }
                }
            }
        }
    }
    Ok(None)
}

fn protected_key_id(path: &Path) -> Result<KeyId> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read RGX private key {}", path.display()))?;
    if text.lines().next().unwrap_or_default().trim() != PROTECTED_PRIVATE_KEY_HEADER {
        bail!("RGX private key is not protected");
    }
    hex_decode::<16>(field(&text, "Key-ID:")?)
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

fn derive_argon2_key(
    password: &str,
    salt: &[u8; 16],
    memory_kib: u32,
    iterations: u32,
    lanes: u32,
) -> Result<Zeroizing<[u8; 32]>> {
    if password.is_empty() {
        bail!("RGX password or key passphrase must not be empty");
    }
    validate_password_parameters(memory_kib, iterations, lanes)?;
    let params = Params::new(memory_kib, iterations, lanes, Some(32))
        .map_err(|error| anyhow!("invalid Argon2 parameters: {error}"))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|error| anyhow!("Argon2id key derivation failed: {error}"))?;
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

fn private_key_file_aad(
    key_id: &KeyId,
    public: &[u8; 32],
    salt: &[u8; 16],
    memory_kib: u32,
    iterations: u32,
    lanes: u32,
) -> Vec<u8> {
    let mut aad = Vec::with_capacity(PRIVATE_KEY_FILE_AAD.len() + 16 + 32 + 16 + 12);
    aad.extend_from_slice(PRIVATE_KEY_FILE_AAD);
    aad.extend_from_slice(key_id);
    aad.extend_from_slice(public);
    aad.extend_from_slice(&memory_kib.to_le_bytes());
    aad.extend_from_slice(&iterations.to_le_bytes());
    aad.extend_from_slice(&lanes.to_le_bytes());
    aad.extend_from_slice(salt);
    aad
}

fn validate_password_parameters(memory_kib: u32, iterations: u32, lanes: u32) -> Result<()> {
    if !(8 * 1024..=1024 * 1024).contains(&memory_kib) {
        bail!("RGX Argon2 memory parameter is outside the accepted range");
    }
    if !(1..=10).contains(&iterations) {
        bail!("RGX Argon2 iteration parameter is outside the accepted range");
    }
    if !(1..=16).contains(&lanes) {
        bail!("RGX Argon2 lane parameter is outside the accepted range");
    }
    Ok(())
}

fn parse_legacy_key_file(text: &str, expected_header: &str) -> Result<[u8; 32]> {
    let mut lines = text.lines();
    let header = lines.next().unwrap_or_default().trim();
    if header != expected_header {
        bail!("not a supported RGX key file ({expected_header} expected)");
    }
    let encoded = lines
        .next()
        .ok_or_else(|| anyhow!("RGX key file is missing key material"))?
        .trim();
    hex_decode::<32>(encoded)
}

fn field<'a>(text: &'a str, label: &str) -> Result<&'a str> {
    text.lines()
        .find_map(|line| line.strip_prefix(label).map(str::trim))
        .ok_or_else(|| anyhow!("RGX key file is missing {label}"))
}

fn parse_u32_field(text: &str, label: &str) -> Result<u32> {
    field(text, label)?
        .parse::<u32>()
        .with_context(|| format!("invalid numeric value for {label}"))
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
        bail!("RGX key material has an unexpected length");
    }
    let mut output = [0u8; N];
    let bytes = value.as_bytes();
    for index in 0..N {
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
    fn key_files_roundtrip_and_include_signing_key() {
        let temp = tempdir().unwrap();
        let private_path = temp.path().join(".ssh/id_rgx");
        let key = RgxPrivateKey::generate();
        let public_path = save_keypair(&private_path, &key).unwrap();

        let loaded_private = load_private_key_with_password(&private_path, None).unwrap();
        let loaded_public = load_public_key(&public_path).unwrap();
        let loaded_signing = load_signing_public_key(&public_path).unwrap();
        assert_eq!(loaded_private.key_id(), key.key_id());
        assert_eq!(loaded_public.key_id(), key.key_id());
        assert_eq!(loaded_signing, key.signing_public_key());
        assert!(save_keypair(&private_path, &key).is_err());
    }

    #[test]
    fn protected_key_file_roundtrip_and_wrong_passphrase_rejection() {
        let temp = tempdir().unwrap();
        let private_path = temp.path().join("protected_id_rgx");
        let key = RgxPrivateKey::generate();
        let public_path =
            save_keypair_protected(&private_path, &key, "strong test passphrase").unwrap();

        assert!(private_key_is_protected(&private_path).unwrap());
        let loaded =
            load_private_key_with_password(&private_path, Some("strong test passphrase")).unwrap();
        assert_eq!(loaded.key_id(), key.key_id());
        assert!(load_private_key_with_password(&private_path, Some("wrong passphrase")).is_err());
        assert!(load_private_key_with_password(&private_path, None).is_err());
        assert_eq!(
            load_signing_public_key(&public_path).unwrap(),
            key.signing_public_key()
        );
    }
}
