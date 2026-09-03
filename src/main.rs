use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use rgx::archive;
use rgx::format::KIND_DIRECTORY;
use rgx::private::{self, ArchiveKind};
use std::path::PathBuf;
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
        /// Read the password from this environment variable instead of prompting.
        #[arg(long, value_name = "NAME")]
        password_env: Option<String>,
    },
    /// Extract an .rgx archive into a new directory.
    Extract {
        archive: PathBuf,
        output: PathBuf,
        /// Read the password from this environment variable instead of prompting.
        #[arg(long, value_name = "NAME")]
        password_env: Option<String>,
    },
    /// List archive contents without extracting them.
    List {
        archive: PathBuf,
        /// Read the password from this environment variable instead of prompting.
        #[arg(long, value_name = "NAME")]
        password_env: Option<String>,
    },
    /// Verify chunk hashes, file hashes, archive structure, and private-envelope authentication.
    Verify {
        archive: PathBuf,
        /// Read the password from this environment variable instead of prompting.
        #[arg(long, value_name = "NAME")]
        password_env: Option<String>,
    },
    /// Show archive statistics.
    Info {
        archive: PathBuf,
        /// Read the password from this environment variable instead of prompting.
        #[arg(long, value_name = "NAME")]
        password_env: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Pack {
            input,
            output,
            level,
            private: private_mode,
            password_env,
        } => {
            let info = if private_mode {
                let password = obtain_password(password_env.as_deref(), true)?;
                private::pack_private(&input, &output, level, password.as_str())?
            } else {
                if password_env.is_some() {
                    bail!("--password-env is only valid together with --private when packing");
                }
                archive::pack(&input, &output, level)?
            };
            println!("Created {}", output.display());
            if private_mode {
                println!("Protection: Argon2id + XChaCha20-Poly1305");
            }
            print_info(&info);
        }
        Commands::Extract {
            archive: archive_path,
            output,
            password_env,
        } => {
            let info = match private::detect_kind(&archive_path)? {
                ArchiveKind::Plain => archive::extract(&archive_path, &output)?,
                ArchiveKind::Private => {
                    let password = obtain_password(password_env.as_deref(), false)?;
                    private::extract_private(&archive_path, &output, password.as_str())?
                }
            };
            println!("Extracted into {}", output.display());
            print_info(&info);
        }
        Commands::List {
            archive: archive_path,
            password_env,
        } => {
            let entries = match private::detect_kind(&archive_path)? {
                ArchiveKind::Plain => archive::list(&archive_path)?,
                ArchiveKind::Private => {
                    let password = obtain_password(password_env.as_deref(), false)?;
                    private::list_private(&archive_path, password.as_str())?
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
            password_env,
        } => {
            let info = match private::detect_kind(&archive_path)? {
                ArchiveKind::Plain => archive::verify(&archive_path)?,
                ArchiveKind::Private => {
                    let password = obtain_password(password_env.as_deref(), false)?;
                    private::verify_private(&archive_path, password.as_str())?
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
            password_env,
        } => {
            let info = match private::detect_kind(&archive_path)? {
                ArchiveKind::Plain => archive::info(&archive_path)?,
                ArchiveKind::Private => {
                    let password = obtain_password(password_env.as_deref(), false)?;
                    private::info_private(&archive_path, password.as_str())?
                }
            };
            print_info(&info);
        }
    }

    Ok(())
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
