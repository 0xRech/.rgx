use anyhow::{anyhow, bail, Context, Result};
use rgx::archive::{self, ArchiveEntry, ArchiveInfo};
use rgx::format::KIND_DIRECTORY;
use rgx::private::{self, ArchiveKind as PrivateArchiveKind};
use rgx::recipient::{self, ArchiveKey};
use rgx::recipient_archive::{self, UnlockMethod};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy)]
enum DetectedKind {
    Plain,
    Private,
    Recipient,
}

impl DetectedKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Private => "private",
            Self::Recipient => "recipient",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackRequest {
    input: String,
    output: String,
    level: i32,
    mode: String,
    password: Option<String>,
    recipient_keys: Vec<String>,
    password_fallback: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccessRequest {
    archive: String,
    identity: Option<String>,
    archive_password: Option<String>,
    key_passphrase: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtractRequest {
    archive: String,
    output_parent: String,
    selected_path: Option<String>,
    identity: Option<String>,
    archive_password: Option<String>,
    key_passphrase: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeygenRequest {
    output: String,
    protect: bool,
    password: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiArchiveInfo {
    version: String,
    entries: u64,
    files: u64,
    directories: u64,
    original_bytes: u64,
    stored_bytes: u64,
    unique_chunks: u64,
    chunk_references: u64,
    deduplicated_bytes: u64,
}

impl From<&ArchiveInfo> for UiArchiveInfo {
    fn from(info: &ArchiveInfo) -> Self {
        Self {
            version: info.version.to_owned(),
            entries: info.entries,
            files: info.files,
            directories: info.directories,
            original_bytes: info.original_bytes,
            stored_bytes: info.stored_bytes,
            unique_chunks: info.unique_chunks,
            chunk_references: info.chunk_references,
            deduplicated_bytes: info.deduplicated_bytes,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackResponse {
    output: String,
    kind: String,
    info: UiArchiveInfo,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExtractResponse {
    output: String,
    kind: String,
    unlock_method: Option<String>,
    info: UiArchiveInfo,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InspectResponse {
    archive: String,
    kind: String,
    unlock_method: Option<String>,
    recipient_count: Option<usize>,
    password_fallback: Option<bool>,
    info: UiArchiveInfo,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VerifyResponse {
    archive: String,
    kind: String,
    unlock_method: Option<String>,
    files: u64,
    unique_chunks: u64,
    chunk_references: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiArchiveEntry {
    path: String,
    kind: String,
    original_size: u64,
    chunks: u32,
}

impl From<ArchiveEntry> for UiArchiveEntry {
    fn from(entry: ArchiveEntry) -> Self {
        Self {
            path: entry.path,
            kind: if entry.kind == KIND_DIRECTORY {
                "directory".to_owned()
            } else {
                "file".to_owned()
            },
            original_size: entry.original_size,
            chunks: entry.chunks,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ListResponse {
    kind: String,
    unlock_method: Option<String>,
    entries: Vec<UiArchiveEntry>,
    total: usize,
    truncated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KeygenResponse {
    private_key: String,
    public_key: String,
    key_id: String,
    protected: bool,
}

#[tauri::command]
async fn pack_archive(request: PackRequest) -> std::result::Result<PackResponse, String> {
    run_blocking(move || pack_archive_blocking(request)).await
}

#[tauri::command]
async fn extract_archive(request: ExtractRequest) -> std::result::Result<ExtractResponse, String> {
    run_blocking(move || extract_archive_blocking(request)).await
}

#[tauri::command]
async fn inspect_archive(request: AccessRequest) -> std::result::Result<InspectResponse, String> {
    run_blocking(move || inspect_archive_blocking(request)).await
}

#[tauri::command]
async fn verify_archive(request: AccessRequest) -> std::result::Result<VerifyResponse, String> {
    run_blocking(move || verify_archive_blocking(request)).await
}

#[tauri::command]
async fn list_archive(request: AccessRequest) -> std::result::Result<ListResponse, String> {
    run_blocking(move || list_archive_blocking(request)).await
}

#[tauri::command]
async fn generate_identity(request: KeygenRequest) -> std::result::Result<KeygenResponse, String> {
    run_blocking(move || generate_identity_blocking(request)).await
}

async fn run_blocking<T, F>(work: F) -> std::result::Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| format!("RGX background task failed: {error}"))?
        .map_err(|error| error.to_string())
}

fn pack_archive_blocking(mut request: PackRequest) -> Result<PackResponse> {
    let input = PathBuf::from(request.input);
    let output = PathBuf::from(request.output);
    let password = take_secret(&mut request.password);

    let (kind, info) = match request.mode.as_str() {
        "plain" => {
            if password.is_some() || request.password_fallback || !request.recipient_keys.is_empty() {
                bail!("plain mode cannot use passwords, fallback, or recipient keys");
            }
            (DetectedKind::Plain, archive::pack(&input, &output, request.level)?)
        }
        "private" => {
            if request.password_fallback || !request.recipient_keys.is_empty() {
                bail!("private mode cannot be combined with recipient options");
            }
            let password = required_secret(password.as_ref(), "a password is required for Private Mode")?;
            (
                DetectedKind::Private,
                private::pack_private(&input, &output, request.level, password)?,
            )
        }
        "recipient" => {
            if request.recipient_keys.is_empty() {
                bail!("Recipient Mode requires at least one RGX public key");
            }
            if request.password_fallback && password.is_none() {
                bail!("password fallback is enabled but no fallback password was provided");
            }
            if !request.password_fallback && password.is_some() {
                bail!("a password was provided but password fallback is not enabled");
            }

            let mut keys = Vec::with_capacity(request.recipient_keys.len());
            for key_path in &request.recipient_keys {
                keys.push(recipient::load_public_key(Path::new(key_path))?);
            }
            let fallback = password.as_ref().map(|value| value.as_str());
            (
                DetectedKind::Recipient,
                recipient_archive::pack_recipient(
                    &input,
                    &output,
                    request.level,
                    &keys,
                    fallback,
                )?,
            )
        }
        other => bail!("unsupported pack mode: {other}"),
    };

    Ok(PackResponse {
        output: output.display().to_string(),
        kind: kind.as_str().to_owned(),
        info: UiArchiveInfo::from(&info),
    })
}

fn extract_archive_blocking(mut request: ExtractRequest) -> Result<ExtractResponse> {
    let archive_path = PathBuf::from(request.archive);
    let output_parent = PathBuf::from(request.output_parent);
    if !output_parent.is_dir() {
        bail!(
            "extraction destination must be an existing directory: {}",
            output_parent.display()
        );
    }

    let selected = normalized_text(request.selected_path.take());
    let identity = normalized_path(request.identity.take());
    let archive_password = take_secret(&mut request.archive_password);
    let key_passphrase = take_secret(&mut request.key_passphrase);
    let kind = detect_archive_kind(&archive_path)?;
    let output = unique_output_dir(&archive_path, &output_parent)?;

    let (info, unlock_method) = match kind {
        DetectedKind::Plain => {
            let info = match selected.as_deref() {
                Some(path) => archive::extract_selected(&archive_path, &output, path)?,
                None => archive::extract(&archive_path, &output)?,
            };
            (info, None)
        }
        DetectedKind::Private => {
            if identity.is_some() || key_passphrase.is_some() {
                bail!("identity options are only valid for recipient-protected archives");
            }
            let password = required_secret(
                archive_password.as_ref(),
                "this Private RGX archive requires its password",
            )?;
            let info = match selected.as_deref() {
                Some(path) => {
                    private::extract_selected_private(&archive_path, &output, path, password)?
                }
                None => private::extract_private(&archive_path, &output, password)?,
            };
            (info, Some("Private password".to_owned()))
        }
        DetectedKind::Recipient => {
            let (key, method) = unlock_recipient(
                &archive_path,
                identity.as_deref(),
                key_passphrase.as_ref().map(|value| value.as_str()),
                archive_password.as_ref().map(|value| value.as_str()),
            )?;
            let info =
                recipient_archive::extract(&archive_path, &output, selected.as_deref(), &key)?;
            (info, Some(method))
        }
    };

    Ok(ExtractResponse {
        output: output.display().to_string(),
        kind: kind.as_str().to_owned(),
        unlock_method,
        info: UiArchiveInfo::from(&info),
    })
}

fn inspect_archive_blocking(mut request: AccessRequest) -> Result<InspectResponse> {
    let archive_path = PathBuf::from(request.archive);
    let identity = normalized_path(request.identity.take());
    let archive_password = take_secret(&mut request.archive_password);
    let key_passphrase = take_secret(&mut request.key_passphrase);
    let kind = detect_archive_kind(&archive_path)?;

    let (info, unlock_method, recipient_count, password_fallback) = match kind {
        DetectedKind::Plain => (archive::info(&archive_path)?, None, None, None),
        DetectedKind::Private => {
            if identity.is_some() || key_passphrase.is_some() {
                bail!("identity options are only valid for recipient-protected archives");
            }
            let password = required_secret(
                archive_password.as_ref(),
                "this Private RGX archive requires its password",
            )?;
            (
                private::info_private(&archive_path, password)?,
                Some("Private password".to_owned()),
                None,
                None,
            )
        }
        DetectedKind::Recipient => {
            let envelope = recipient_archive::read_envelope(&archive_path)?;
            let recipient_count = envelope.recipients.len();
            let has_fallback = envelope.password.is_some();
            let (key, method) = unlock_recipient(
                &archive_path,
                identity.as_deref(),
                key_passphrase.as_ref().map(|value| value.as_str()),
                archive_password.as_ref().map(|value| value.as_str()),
            )?;
            (
                recipient_archive::info(&archive_path, &key)?,
                Some(method),
                Some(recipient_count),
                Some(has_fallback),
            )
        }
    };

    Ok(InspectResponse {
        archive: archive_path.display().to_string(),
        kind: kind.as_str().to_owned(),
        unlock_method,
        recipient_count,
        password_fallback,
        info: UiArchiveInfo::from(&info),
    })
}

fn verify_archive_blocking(mut request: AccessRequest) -> Result<VerifyResponse> {
    let archive_path = PathBuf::from(request.archive);
    let identity = normalized_path(request.identity.take());
    let archive_password = take_secret(&mut request.archive_password);
    let key_passphrase = take_secret(&mut request.key_passphrase);
    let kind = detect_archive_kind(&archive_path)?;

    let (info, unlock_method) = match kind {
        DetectedKind::Plain => (archive::verify(&archive_path)?, None),
        DetectedKind::Private => {
            if identity.is_some() || key_passphrase.is_some() {
                bail!("identity options are only valid for recipient-protected archives");
            }
            let password = required_secret(
                archive_password.as_ref(),
                "this Private RGX archive requires its password",
            )?;
            (
                private::verify_private(&archive_path, password)?,
                Some("Private password".to_owned()),
            )
        }
        DetectedKind::Recipient => {
            let (key, method) = unlock_recipient(
                &archive_path,
                identity.as_deref(),
                key_passphrase.as_ref().map(|value| value.as_str()),
                archive_password.as_ref().map(|value| value.as_str()),
            )?;
            (recipient_archive::verify(&archive_path, &key)?, Some(method))
        }
    };

    Ok(VerifyResponse {
        archive: archive_path.display().to_string(),
        kind: kind.as_str().to_owned(),
        unlock_method,
        files: info.files,
        unique_chunks: info.unique_chunks,
        chunk_references: info.chunk_references,
    })
}

fn list_archive_blocking(mut request: AccessRequest) -> Result<ListResponse> {
    const UI_ENTRY_LIMIT: usize = 1_000;

    let archive_path = PathBuf::from(request.archive);
    let identity = normalized_path(request.identity.take());
    let archive_password = take_secret(&mut request.archive_password);
    let key_passphrase = take_secret(&mut request.key_passphrase);
    let kind = detect_archive_kind(&archive_path)?;

    let (entries, unlock_method) = match kind {
        DetectedKind::Plain => (archive::list(&archive_path)?, None),
        DetectedKind::Private => {
            if identity.is_some() || key_passphrase.is_some() {
                bail!("identity options are only valid for recipient-protected archives");
            }
            let password = required_secret(
                archive_password.as_ref(),
                "this Private RGX archive requires its password",
            )?;
            (
                private::list_private(&archive_path, password)?,
                Some("Private password".to_owned()),
            )
        }
        DetectedKind::Recipient => {
            let (key, method) = unlock_recipient(
                &archive_path,
                identity.as_deref(),
                key_passphrase.as_ref().map(|value| value.as_str()),
                archive_password.as_ref().map(|value| value.as_str()),
            )?;
            (recipient_archive::list(&archive_path, &key)?, Some(method))
        }
    };

    let total = entries.len();
    let truncated = total > UI_ENTRY_LIMIT;
    let entries = entries
        .into_iter()
        .take(UI_ENTRY_LIMIT)
        .map(UiArchiveEntry::from)
        .collect();

    Ok(ListResponse {
        kind: kind.as_str().to_owned(),
        unlock_method,
        entries,
        total,
        truncated,
    })
}

fn generate_identity_blocking(mut request: KeygenRequest) -> Result<KeygenResponse> {
    let private_path = PathBuf::from(request.output);
    let password = take_secret(&mut request.password);

    let (key_id, public_path) = if request.protect {
        let password = required_secret(
            password.as_ref(),
            "a passphrase is required for a protected RGX identity",
        )?;
        recipient::generate_and_save_keypair_protected(&private_path, password)?
    } else {
        if password.is_some() {
            bail!("a passphrase was provided but identity protection is disabled");
        }
        recipient::generate_and_save_keypair(&private_path)?
    };

    Ok(KeygenResponse {
        private_key: private_path.display().to_string(),
        public_key: public_path.display().to_string(),
        key_id: recipient::format_key_id(&key_id),
        protected: request.protect,
    })
}

fn detect_archive_kind(path: &Path) -> Result<DetectedKind> {
    let mut file =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)
        .with_context(|| format!("failed to read RGX header from {}", path.display()))?;

    if magic == recipient_archive::RECIPIENT_MAGIC {
        return Ok(DetectedKind::Recipient);
    }

    match private::detect_kind(path)? {
        PrivateArchiveKind::Plain => Ok(DetectedKind::Plain),
        PrivateArchiveKind::Private => Ok(DetectedKind::Private),
    }
}

fn unlock_recipient(
    archive_path: &Path,
    identity: Option<&Path>,
    key_passphrase: Option<&str>,
    archive_password: Option<&str>,
) -> Result<(Zeroizing<ArchiveKey>, String)> {
    if let Some(identity_path) = identity {
        let private_key =
            recipient::load_private_key_with_password(identity_path, key_passphrase)?;
        let envelope = recipient_archive::read_envelope(archive_path)?;
        let key_id = private_key.key_id();
        let slot = envelope
            .recipients
            .iter()
            .find(|slot| slot.key_id == key_id)
            .ok_or_else(|| anyhow!("the selected RGX identity is not a recipient of this archive"))?;
        let archive_key = recipient::unwrap_archive_key_for_recipient(slot, &private_key)?;
        return Ok((
            archive_key,
            format!(
                "Identity {} ({})",
                identity_path.display(),
                recipient::format_key_id(&key_id)
            ),
        ));
    }

    if key_passphrase.is_some() {
        bail!("select the protected RGX identity file when providing a key passphrase");
    }

    let mut discovery_error = None;
    match recipient_archive::try_unlock_identity(archive_path, None) {
        Ok(Some((key, method))) => return Ok((key, describe_unlock_method(method))),
        Ok(None) => {}
        Err(error) => discovery_error = Some(error),
    }

    if let Some(password) = archive_password {
        let (key, method) = recipient_archive::unlock_password(archive_path, password)?;
        return Ok((key, describe_unlock_method(method)));
    }

    if let Some(error) = discovery_error {
        return Err(error).context(
            "automatic RGX identity unlock failed; select an identity explicitly or use password fallback",
        );
    }

    let envelope = recipient_archive::read_envelope(archive_path)?;
    if envelope.password.is_some() {
        bail!("no matching RGX identity was found; this archive supports a password fallback");
    }
    bail!("no matching RGX identity was found for this recipient archive")
}

fn describe_unlock_method(method: UnlockMethod) -> String {
    match method {
        UnlockMethod::Identity { path, key_id } => format!(
            "Identity {} ({})",
            path.display(),
            recipient::format_key_id(&key_id)
        ),
        UnlockMethod::PasswordFallback => "Recipient password fallback".to_owned(),
    }
}

fn unique_output_dir(archive: &Path, parent: &Path) -> Result<PathBuf> {
    let stem = archive
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("RGX-Extracted");

    let base = parent.join(stem);
    if !base.exists() {
        return Ok(base);
    }

    for suffix in 2u32.. {
        let candidate = parent.join(format!("{stem}-{suffix}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    unreachable!("u32 output suffix space exhausted")
}

fn normalized_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

fn normalized_path(value: Option<String>) -> Option<PathBuf> {
    normalized_text(value).map(PathBuf::from)
}

fn take_secret(value: &mut Option<String>) -> Option<Zeroizing<String>> {
    value.take().and_then(|value| {
        if value.is_empty() {
            None
        } else {
            Some(Zeroizing::new(value))
        }
    })
}

fn required_secret<'a>(
    value: Option<&'a Zeroizing<String>>,
    message: &str,
) -> Result<&'a str> {
    value
        .map(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!(message.to_owned()))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            pack_archive,
            extract_archive,
            inspect_archive,
            verify_archive,
            list_archive,
            generate_identity
        ])
        .run(tauri::generate_context!())
        .expect("error while running RGX Desktop");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn output_directory_is_unique_and_keeps_archive_name() {
        let temp = tempdir().unwrap();
        let archive = temp.path().join("project archive.rgx");

        let first = unique_output_dir(&archive, temp.path()).unwrap();
        assert_eq!(first, temp.path().join("project archive"));

        fs::create_dir(&first).unwrap();
        let second = unique_output_dir(&archive, temp.path()).unwrap();
        assert_eq!(second, temp.path().join("project archive-2"));
    }

    #[test]
    fn recipient_kind_is_detected_without_private_parser_fallback() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("data.txt");
        fs::write(&source, b"desktop recipient detection").unwrap();

        let private_key = temp.path().join("id_rgx");
        let (_, public_key) = recipient::generate_and_save_keypair(&private_key).unwrap();
        let public = recipient::load_public_key(&public_key).unwrap();

        let archive = temp.path().join("recipient.rgx");
        recipient_archive::pack_recipient(&source, &archive, 3, &[public], None).unwrap();

        assert!(matches!(
            detect_archive_kind(&archive).unwrap(),
            DetectedKind::Recipient
        ));
    }
}
