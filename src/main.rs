use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use rgx::archive;
use rgx::benchmark::{self, BenchmarkOptions};
use rgx::format::KIND_DIRECTORY;
use rgx::private::{self, ArchiveKind};
use std::io::{self, Write};
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
        /// Extract only this file or directory subtree.
        #[arg(long, value_name = "ARCHIVE_PATH")]
        path: Option<String>,
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
    /// Find files and directories by case-insensitive path substring.
    Find {
        archive: PathBuf,
        query: String,
        #[arg(long, value_name = "NAME")]
        password_env: Option<String>,
    },
    /// Write one archived file to standard output.
    Cat {
        archive: PathBuf,
        path: String,
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
            path,
            password_env,
        } => {
            let info = match private::detect_kind(&archive_path)? {
                ArchiveKind::Plain => match path.as_deref() {
                    Some(selected) => archive::extract_selected(&archive_path, &output, selected)?,
                    None => archive::extract(&archive_path, &output)?,
                },
                ArchiveKind::Private => {
                    let password = obtain_password(password_env.as_deref(), false)?;
                    match path.as_deref() {
                        Some(selected) => private::extract_selected_private(
                            &archive_path, &output, selected, password.as_str()
                        )?,
                        None => private::extract_private(&archive_path, &output, password.as_str())?,
                    }
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
        Commands::Find {
            archive: archive_path,
            query,
            password_env,
        } => {
            let entries = match private::detect_kind(&archive_path)? {
                ArchiveKind::Plain => archive::find(&archive_path, &query)?,
                ArchiveKind::Private => {
                    let password = obtain_password(password_env.as_deref(), false)?;
                    private::find_private(&archive_path, &query, password.as_str())?
                }
            };
            for entry in entries {
                println!("{}", entry.path);
            }
        }
        Commands::Cat {
            archive: archive_path,
            path,
            password_env,
        } => {
            let data = match private::detect_kind(&archive_path)? {
                ArchiveKind::Plain => archive::read_entry(&archive_path, &path)?,
                ArchiveKind::Private => {
                    let password = obtain_password(password_env.as_deref(), false)?;
                    private::read_entry_private(&archive_path, &path, password.as_str())?
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
