use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use rgx::archive;
use rgx::benchmark::{self, BenchmarkOptions};
use rgx::format::KIND_DIRECTORY;
use rgx::private::{self, ArchiveKind};
use rgx::recipient::{self, ArchiveKey};
use rgx::recipient_archive::{self, UnlockMethod};
use rgx::signature;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

#[derive(Parser, Debug)]
#[command(name = "rgx")]
#[command(version)]
#[command(about = ".rgx — compact, private, resilient archives")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create a new .rgx archive.
    Pack {
        /// File or directory to archive.
        input: PathBuf,
        /// Destination .rgx file.
        output: PathBuf,
        /// Zstandard compression level (1-22).
        #[arg(short, long, default_value_t = 3)]
        level: i32,
        /// Protect the complete RGX container with Argon2id + XChaCha20-Poly1305.
        #[arg(long)]
        private: bool,
        /// Encrypt for an RGX recipient public key. May be specified multiple times.
        #[arg(long, value_name = "PUBLIC_KEY")]
        recipient: Vec<PathBuf>,
        /// Add a password fallback to a recipient-protected archive.
        #[arg(long)]
        password_fallback: bool,
        /// Read the password from this environment variable instead of prompting.
        #[arg(long, value_name = "NAME")]
        password_env: Option<String>,
    },
    /// Generate an X25519 + Ed25519 RGX identity. Defaults to ~/.ssh/id_rgx.
    Keygen {
        /// Private-key destination. The public key is written with a .pub suffix.
        #[arg(short, long, value_name = "PATH")]
        output: Option<PathBuf>,
        /// Protect the private-key file with Argon2id + XChaCha20-Poly1305.
        #[arg(long)]
        protect: bool,
        /// Read the key-protection passphrase from this environment variable.
        #[arg(long, value_name = "NAME")]
        password_env: Option<String>,
    },
    /// Create a detached Ed25519 signature for an RGX archive.
    Sign {
        archive: PathBuf,
        /// RGX private identity. Defaults to ~/.ssh/id_rgx.
        #[arg(long, value_name = "PRIVATE_KEY")]
        identity: Option<PathBuf>,
        /// Signature destination. Defaults to ARCHIVE.sig.
        #[arg(short, long, value_name = "PATH")]
        output: Option<PathBuf>,
        /// Read a protected identity passphrase from this environment variable.
        #[arg(long, value_name = "NAME")]
        key_password_env: Option<String>,
    },
    /// Verify a detached RGX Ed25519 signature.
    VerifySignature {
        archive: PathBuf,
        signature: PathBuf,
        /// Trusted RGX public key containing the Ed25519 verification key.
        #[arg(long, value_name = "PUBLIC_KEY")]
        public_key: PathBuf,
    },
    /// Extract an .rgx archive into a new directory.
    Extract {
        archive: PathBuf,
        output: PathBuf,
        /// Extract only this file or directory subtree.
        #[arg(long, value_name = "ARCHIVE_PATH")]
        path: Option<String>,
        /// Explicit RGX private identity. Otherwise default key locations are searched.
        #[arg(long, value_name = "PRIVATE_KEY")]
        identity: Option<PathBuf>,
        /// Read the password from this environment variable instead of prompting.
        #[arg(long, value_name = "NAME")]
        password_env: Option<String>,
    },
    /// List archive contents without extracting them.
    List {
        archive: PathBuf,
        #[arg(long, value_name = "PRIVATE_KEY")]
        identity: Option<PathBuf>,
        /// Read the password from this environment variable instead of prompting.
        #[arg(long, value_name = "NAME")]
        password_env: Option<String>,
    },
    /// Verify chunk hashes, file hashes, archive structure, and envelope authentication.
    Verify {
        archive: PathBuf,
        #[arg(long, value_name = "PRIVATE_KEY")]
        identity: Option<PathBuf>,
        /// Read the password from this environment variable instead of prompting.
        #[arg(long, value_name = "NAME")]
        password_env: Option<String>,
    },
    /// Show archive statistics.
    Info {
        archive: PathBuf,
        #[arg(long, value_name = "PRIVATE_KEY")]
        identity: Option<PathBuf>,
        /// Read the password from this environment variable instead of prompting.
        #[arg(long, value_name = "NAME")]
        password_env: Option<String>,
    },
    /// Find files and directories by case-insensitive path substring.
    Find {
        archive: PathBuf,
        query: String,
        #[arg(long, value_name = "PRIVATE_KEY")]
        identity: Option<PathBuf>,
        #[arg(long, value_name = "NAME")]
        password_env: Option<String>,
    },
    /// Write one archived file to standard output.
    Cat {
        archive: PathBuf,
        path: String,
        #[arg(long, value_name = "PRIVATE_KEY")]
        identity: Option<PathBuf>,
        #[arg(long, value_name = "NAME")]
        password_env: Option<String>,
    },
    /// Compare RGX pack/extract speed and archive size with ZIP and optionally 7-Zip.
    Benchmark {
        /// File or directory to benchmark.
        input: PathBuf,
        /// Zstandard compression level used for RGX (1-22).
        #[arg(short, long, default_value_t = 3)]
        level: i32,
        /// Also benchmark RGX Private Mode. A temporary internal benchmark password is used.
        #[arg(long)]
        private: bool,
        /// Do not try to benchmark an installed 7z/7zz/7za executable.
        #[arg(long)]
        no_7zip: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetectedKind {
    Plain,
    Private,
    Recipient,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Pack {
            input,
            output,
            level,
            private: private_mode,
            recipient: recipient_paths,
            password_fallback,
            password_env,
        } => {
            let recipient_mode = !recipient_paths.is_empty();
            if recipient_mode && private_mode {
                bail!("--private and --recipient cannot be combined; recipient archives are already encrypted");
            }
            if password_fallback && !recipient_mode {
                bail!("--password-fallback is only valid together with --recipient");
            }

            let info = if recipient_mode {
                let mut public_keys = Vec::with_capacity(recipient_paths.len());
                for path in &recipient_paths {
                    public_keys.push(recipient::load_public_key(path)?);
                }
                let fallback_password = if password_fallback {
                    Some(obtain_password(password_env.as_deref(), true)?)
                } else {
                    if password_env.is_some() {
                        bail!("--password-env requires --password-fallback when packing for recipients");
                    }
                    None
                };
                recipient_archive::pack_recipient(
                    &input,
                    &output,
                    level,
                    &public_keys,
                    fallback_password.as_ref().map(|password| password.as_str()),
                )?
            } else if private_mode {
                let password = obtain_password(password_env.as_deref(), true)?;
                private::pack_private(&input, &output, level, password.as_str())?
            } else {
                if password_env.is_some() {
                    bail!("--password-env is only valid with --private or recipient password fallback");
                }
                archive::pack(&input, &output, level)?
            };

            println!("Created {}", output.display());
            if recipient_mode {
                println!("Protection: X25519 recipients + XChaCha20-Poly1305");
                println!("Recipients: {}", recipient_paths.len());
                if password_fallback {
                    println!("Password fallback: enabled (Argon2id)");
                }
            } else if private_mode {
                println!("Protection: Argon2id + XChaCha20-Poly1305");
            }
            print_info(&info);
        }
        Commands::Keygen {
            output,
            protect,
            password_env,
        } => {
            let private_path = match output {
                Some(path) => path,
                None => recipient::default_private_key_path()?,
            };
            let (key_id, public_path) = if protect {
                let password = obtain_password(password_env.as_deref(), true)?;
                recipient::generate_and_save_keypair_protected(&private_path, password.as_str())?
            } else {
                if password_env.is_some() {
                    bail!("--password-env requires --protect when generating an RGX identity");
                }
                recipient::generate_and_save_keypair(&private_path)?
            };
            println!("Created private key: {}", private_path.display());
            println!("Created public key:  {}", public_path.display());
            println!("Key-ID: {}", recipient::format_key_id(&key_id));
            println!("Recipient key: X25519");
            println!("Signing key: Ed25519");
            println!(
                "Private-key protection: {}",
                if protect {
                    "Argon2id + XChaCha20-Poly1305"
                } else {
                    "filesystem permissions"
                }
            );
        }
        Commands::Sign {
            archive,
            identity,
            output,
            key_password_env,
        } => {
            let identity_path = match identity {
                Some(path) => path,
                None => recipient::default_private_key_path()?,
            };
            let private_key = if recipient::private_key_is_protected(&identity_path)? {
                let passphrase = obtain_key_passphrase(key_password_env.as_deref())?;
                recipient::load_private_key_with_password(
                    &identity_path,
                    Some(passphrase.as_str()),
                )?
            } else {
                if key_password_env.is_some() {
                    bail!("--key-password-env is only valid for a protected RGX identity");
                }
                recipient::load_private_key(&identity_path)?
            };
            let signature_path =
                output.unwrap_or_else(|| signature::default_signature_path(&archive));
            let info = signature::sign_archive(&archive, &private_key, &signature_path)?;
            println!("Created signature: {}", signature_path.display());
            println!("Signer Key-ID: {}", recipient::format_key_id(&info.key_id));
            println!(
                "Archive BLAKE3: {}",
                signature::format_hash(&info.archive_hash)
            );
        }
        Commands::VerifySignature {
            archive,
            signature: signature_path,
            public_key,
        } => {
            let info = signature::verify_archive_signature(&archive, &signature_path, &public_key)?;
            println!("Signature OK: {}", archive.display());
            println!("Signer Key-ID: {}", recipient::format_key_id(&info.key_id));
            println!(
                "Archive BLAKE3: {}",
                signature::format_hash(&info.archive_hash)
            );
        }
        Commands::Extract {
            archive: archive_path,
            output,
            path,
            identity,
            password_env,
        } => {
            let info = match detect_archive_kind(&archive_path)? {
                DetectedKind::Plain => match path.as_deref() {
                    Some(selected) => archive::extract_selected(&archive_path, &output, selected)?,
                    None => archive::extract(&archive_path, &output)?,
                },
                DetectedKind::Private => {
                    if identity.is_some() {
                        bail!("--identity is only valid for recipient-protected RGX archives");
                    }
                    let password = obtain_password(password_env.as_deref(), false)?;
                    match path.as_deref() {
                        Some(selected) => private::extract_selected_private(
                            &archive_path,
                            &output,
                            selected,
                            password.as_str(),
                        )?,
                        None => {
                            private::extract_private(&archive_path, &output, password.as_str())?
                        }
                    }
                }
                DetectedKind::Recipient => {
                    let (key, method) = unlock_recipient(
                        &archive_path,
                        identity.as_deref(),
                        password_env.as_deref(),
                    )?;
                    print_unlock_method(&method);
                    recipient_archive::extract(&archive_path, &output, path.as_deref(), &key)?
                }
            };
            println!("Extracted into {}", output.display());
            print_info(&info);
        }
        Commands::List {
            archive: archive_path,
            identity,
            password_env,
        } => {
            let entries = match detect_archive_kind(&archive_path)? {
                DetectedKind::Plain => archive::list(&archive_path)?,
                DetectedKind::Private => {
                    reject_identity_for_nonrecipient(identity.as_deref())?;
                    let password = obtain_password(password_env.as_deref(), false)?;
                    private::list_private(&archive_path, password.as_str())?
                }
                DetectedKind::Recipient => {
                    let (key, _) = unlock_recipient(
                        &archive_path,
                        identity.as_deref(),
                        password_env.as_deref(),
                    )?;
                    recipient_archive::list(&archive_path, &key)?
                }
            };
            for entry in entries {
                if entry.kind == KIND_DIRECTORY {
                    println!("DIR   {}", entry.path);
                } else {
                    println!(
                        "FILE  {:>12} B  {:>7} chunks  {}",
                        entry.original_size, entry.chunks, entry.path
                    );
                }
            }
        }
        Commands::Verify {
            archive: archive_path,
            identity,
            password_env,
        } => {
            let info = match detect_archive_kind(&archive_path)? {
                DetectedKind::Plain => archive::verify(&archive_path)?,
                DetectedKind::Private => {
                    reject_identity_for_nonrecipient(identity.as_deref())?;
                    let password = obtain_password(password_env.as_deref(), false)?;
                    private::verify_private(&archive_path, password.as_str())?
                }
                DetectedKind::Recipient => {
                    let (key, method) = unlock_recipient(
                        &archive_path,
                        identity.as_deref(),
                        password_env.as_deref(),
                    )?;
                    print_unlock_method(&method);
                    recipient_archive::verify(&archive_path, &key)?
                }
            };
            println!("OK: {}", archive_path.display());
            println!(
                "Verified {} files, {} unique chunks, and {} chunk references.",
                info.files, info.unique_chunks, info.chunk_references
            );
        }
        Commands::Info {
            archive: archive_path,
            identity,
            password_env,
        } => {
            let info = match detect_archive_kind(&archive_path)? {
                DetectedKind::Plain => archive::info(&archive_path)?,
                DetectedKind::Private => {
                    reject_identity_for_nonrecipient(identity.as_deref())?;
                    let password = obtain_password(password_env.as_deref(), false)?;
                    private::info_private(&archive_path, password.as_str())?
                }
                DetectedKind::Recipient => {
                    let (key, method) = unlock_recipient(
                        &archive_path,
                        identity.as_deref(),
                        password_env.as_deref(),
                    )?;
                    print_unlock_method(&method);
                    let envelope = recipient_archive::read_envelope(&archive_path)?;
                    println!(
                        "Recipient envelope: v{}",
                        recipient_archive::RECIPIENT_VERSION
                    );
                    println!("Recipient slots: {}", envelope.recipients.len());
                    println!(
                        "Password fallback: {}",
                        if envelope.password.is_some() {
                            "yes"
                        } else {
                            "no"
                        }
                    );
                    recipient_archive::info(&archive_path, &key)?
                }
            };
            print_info(&info);
        }
        Commands::Find {
            archive: archive_path,
            query,
            identity,
            password_env,
        } => {
            let entries = match detect_archive_kind(&archive_path)? {
                DetectedKind::Plain => archive::find(&archive_path, &query)?,
                DetectedKind::Private => {
                    reject_identity_for_nonrecipient(identity.as_deref())?;
                    let password = obtain_password(password_env.as_deref(), false)?;
                    private::find_private(&archive_path, &query, password.as_str())?
                }
                DetectedKind::Recipient => {
                    let (key, _) = unlock_recipient(
                        &archive_path,
                        identity.as_deref(),
                        password_env.as_deref(),
                    )?;
                    recipient_archive::find(&archive_path, &query, &key)?
                }
            };
            for entry in entries {
                println!("{}", entry.path);
            }
        }
        Commands::Cat {
            archive: archive_path,
            path,
            identity,
            password_env,
        } => {
            let data = match detect_archive_kind(&archive_path)? {
                DetectedKind::Plain => archive::read_entry(&archive_path, &path)?,
                DetectedKind::Private => {
                    reject_identity_for_nonrecipient(identity.as_deref())?;
                    let password = obtain_password(password_env.as_deref(), false)?;
                    private::read_entry_private(&archive_path, &path, password.as_str())?
                }
                DetectedKind::Recipient => {
                    let (key, _) = unlock_recipient(
                        &archive_path,
                        identity.as_deref(),
                        password_env.as_deref(),
                    )?;
                    recipient_archive::read_entry(&archive_path, &path, &key)?
                }
            };
            io::stdout().lock().write_all(&data)?;
        }
        Commands::Benchmark {
            input,
            level,
            private: include_private,
            no_7zip,
        } => {
            let options = BenchmarkOptions {
                level,
                include_private,
                include_7zip: !no_7zip,
            };
            let report = benchmark::run(&input, &options)?;
            print_benchmark(&report);
        }
    }

    Ok(())
}

fn detect_archive_kind(path: &Path) -> Result<DetectedKind> {
    let mut file =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;
    if magic == recipient_archive::RECIPIENT_MAGIC {
        return Ok(DetectedKind::Recipient);
    }
    match private::detect_kind(path)? {
        ArchiveKind::Plain => Ok(DetectedKind::Plain),
        ArchiveKind::Private => Ok(DetectedKind::Private),
    }
}

fn unlock_recipient(
    archive_path: &Path,
    identity: Option<&Path>,
    password_env: Option<&str>,
) -> Result<(Zeroizing<ArchiveKey>, UnlockMethod)> {
    if let Some(unlocked) = recipient_archive::try_unlock_identity(archive_path, identity)? {
        return Ok(unlocked);
    }
    if !recipient_archive::has_password_fallback(archive_path)? {
        bail!("no matching RGX recipient key was found and this archive has no password fallback");
    }
    let password = obtain_password(password_env, false)?;
    recipient_archive::unlock_password(archive_path, password.as_str())
}

fn print_unlock_method(method: &UnlockMethod) {
    match method {
        UnlockMethod::Identity { path, key_id } => {
            println!("Unlocked with RGX identity: {}", path.display());
            println!("Recipient Key-ID: {}", recipient::format_key_id(key_id));
        }
        UnlockMethod::PasswordFallback => println!("Unlocked with password fallback"),
    }
}

fn reject_identity_for_nonrecipient(identity: Option<&Path>) -> Result<()> {
    if identity.is_some() {
        bail!("--identity is only valid for recipient-protected RGX archives");
    }
    Ok(())
}

fn obtain_key_passphrase(environment_name: Option<&str>) -> Result<Zeroizing<String>> {
    if let Some(name) = environment_name {
        let value = std::env::var(name)
            .with_context(|| format!("environment variable {name} is not set"))?;
        if value.is_empty() {
            bail!("RGX key passphrase environment variable must not be empty");
        }
        return Ok(Zeroizing::new(value));
    }

    let password = Zeroizing::new(rpassword::prompt_password("RGX key passphrase: ")?);
    if password.is_empty() {
        bail!("RGX key passphrase must not be empty");
    }
    Ok(password)
}
fn obtain_password(environment_name: Option<&str>, confirm: bool) -> Result<Zeroizing<String>> {
    if let Some(name) = environment_name {
        let value = std::env::var(name)
            .with_context(|| format!("environment variable {name} is not set"))?;
        if value.is_empty() {
            bail!("password environment variable must not be empty");
        }
        return Ok(Zeroizing::new(value));
    }

    let password = Zeroizing::new(rpassword::prompt_password("RGX password: ")?);
    if password.is_empty() {
        bail!("private RGX password must not be empty");
    }
    if confirm {
        let confirmation = Zeroizing::new(rpassword::prompt_password("Confirm RGX password: ")?);
        if password.as_str() != confirmation.as_str() {
            bail!("password confirmation does not match");
        }
    }
    Ok(password)
}

fn print_info(info: &archive::ArchiveInfo) {
    println!("RGX data format: {}", info.version);
    println!("Entries: {}", info.entries);
    println!("Files: {}", info.files);
    println!("Directories: {}", info.directories);
    println!("Unique chunks: {}", info.unique_chunks);
    println!("Chunk references: {}", info.chunk_references);
    println!("Original bytes: {}", info.original_bytes);
    println!("Stored chunk payload bytes: {}", info.stored_bytes);
    println!("Deduplicated logical bytes: {}", info.deduplicated_bytes);
    if info.original_bytes > 0 {
        let payload_ratio = info.stored_bytes as f64 / info.original_bytes as f64 * 100.0;
        let dedup_ratio = info.deduplicated_bytes as f64 / info.original_bytes as f64 * 100.0;
        println!("Payload ratio: {payload_ratio:.2}%");
        println!("Deduplicated share: {dedup_ratio:.2}%");
    }
}

fn print_benchmark(report: &benchmark::BenchmarkReport) {
    println!("RGX Benchmark");
    println!(
        "Input: {} / {} files",
        human_bytes(report.input_bytes),
        report.files
    );
    println!();
    println!(
        "{:<24} {:>12} {:>10} {:>10} {:>12} {:>12}",
        "Method", "Size", "Pack", "Extract", "Pack MiB/s", "Extr MiB/s"
    );
    println!("{}", "-".repeat(86));

    for result in &report.results {
        println!(
            "{:<24} {:>12} {:>9.2}s {:>9.2}s {:>12.1} {:>12.1}",
            result.name,
            human_bytes(result.archive_bytes),
            result.pack_time.as_secs_f64(),
            result.extract_time.as_secs_f64(),
            result.pack_mib_per_second(report.input_bytes),
            result.extract_mib_per_second(report.input_bytes)
        );
    }

    println!();
    println!(
        "RGX deduplicated logical data: {}",
        human_bytes(report.rgx_deduplicated_bytes)
    );
    if report.input_bytes > 0 {
        let share = report.rgx_deduplicated_bytes as f64 / report.input_bytes as f64 * 100.0;
        println!("RGX deduplicated share: {share:.2}%");
    }
    for skipped in &report.skipped {
        println!("Skipped: {skipped}");
    }
    println!(
        "Note: timings are wall-clock measurements on this machine; ZIP uses Deflate defaults and 7-Zip uses -mx=5."
    );
}

fn human_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * KIB;
    const GIB: f64 = 1024.0 * MIB;
    let value = bytes as f64;
    if value >= GIB {
        format!("{:.2} GiB", value / GIB)
    } else if value >= MIB {
        format!("{:.2} MiB", value / MIB)
    } else if value >= KIB {
        format!("{:.2} KiB", value / KIB)
    } else {
        format!("{bytes} B")
    }
}
