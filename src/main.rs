use anyhow::Result;
use clap::{Parser, Subcommand};
use rgx::archive;
use rgx::format::{COMPRESSION_STORE, COMPRESSION_ZSTD, KIND_DIRECTORY};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "rgx")]
#[command(version)]
#[command(about = ".rgx — compact, private-ready, resilient archives")]
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
    },
    /// Extract an .rgx archive into a new directory.
    Extract {
        archive: PathBuf,
        output: PathBuf,
    },
    /// List archive contents without extracting them.
    List { archive: PathBuf },
    /// Verify all file hashes and archive structure.
    Verify { archive: PathBuf },
    /// Show archive statistics.
    Info { archive: PathBuf },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Pack {
            input,
            output,
            level,
        } => {
            let info = archive::pack(&input, &output, level)?;
            println!("Created {}", output.display());
            print_info(&info);
        }
        Commands::Extract { archive, output } => {
            let info = archive::extract(&archive, &output)?;
            println!("Extracted into {}", output.display());
            print_info(&info);
        }
        Commands::List { archive } => {
            let entries = archive::list(&archive)?;
            for entry in entries {
                if entry.kind == KIND_DIRECTORY {
                    println!("DIR   {}", entry.path);
                } else {
                    let codec = match entry.compression {
                        COMPRESSION_STORE => "store",
                        COMPRESSION_ZSTD => "zstd",
                        _ => "unknown",
                    };
                    println!(
                        "FILE  {:>10} B  {:>10} B  {:<5}  {}",
                        entry.original_size, entry.payload_size, codec, entry.path
                    );
                }
            }
        }
        Commands::Verify { archive } => {
            let info = archive::verify(&archive)?;
            println!("OK: {}", archive.display());
            println!("Verified {} files across {} entries.", info.files, info.entries);
        }
        Commands::Info { archive } => {
            let info = archive::info(&archive)?;
            print_info(&info);
        }
    }

    Ok(())
}

fn print_info(info: &archive::ArchiveInfo) {
    println!("RGX version: {}", info.version);
    println!("Entries: {}", info.entries);
    println!("Files: {}", info.files);
    println!("Directories: {}", info.directories);
    println!("Original bytes: {}", info.original_bytes);
    println!("Stored payload bytes: {}", info.stored_bytes);
    if info.original_bytes > 0 {
        let ratio = info.stored_bytes as f64 / info.original_bytes as f64 * 100.0;
        println!("Payload ratio: {ratio:.2}%");
    }
}
