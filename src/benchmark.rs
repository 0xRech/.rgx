use crate::{archive, private};
use anyhow::{anyhow, bail, Context, Result};
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter};
use std::path::{Component, Path};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::tempdir;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const BENCHMARK_PRIVATE_PASSWORD: &str = "rgx-benchmark-temporary-password";

#[derive(Debug, Clone)]
pub struct BenchmarkOptions {
    pub level: i32,
    pub include_private: bool,
    pub include_7zip: bool,
}

impl Default for BenchmarkOptions {
    fn default() -> Self {
        Self {
            level: 3,
            include_private: false,
            include_7zip: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub name: String,
    pub archive_bytes: u64,
    pub pack_time: Duration,
    pub extract_time: Duration,
}

impl BenchmarkResult {
    pub fn pack_mib_per_second(&self, input_bytes: u64) -> f64 {
        throughput(input_bytes, self.pack_time)
    }

    pub fn extract_mib_per_second(&self, input_bytes: u64) -> f64 {
        throughput(input_bytes, self.extract_time)
    }
}

#[derive(Debug, Clone)]
pub struct BenchmarkReport {
    pub input_bytes: u64,
    pub files: u64,
    pub rgx_deduplicated_bytes: u64,
    pub results: Vec<BenchmarkResult>,
    pub skipped: Vec<String>,
}

pub fn run(input: &Path, options: &BenchmarkOptions) -> Result<BenchmarkReport> {
    if !input.exists() {
        bail!("input does not exist: {}", input.display());
    }
    if !(1..=22).contains(&options.level) {
        bail!("zstd level must be between 1 and 22");
    }

    let (input_bytes, files) = source_stats(input)?;
    let workspace = tempdir().context("failed to create benchmark workspace")?;
    let mut results = Vec::new();
    let mut skipped = Vec::new();

    let rgx_path = workspace.path().join("benchmark.rgx");
    let started = Instant::now();
    let rgx_info = archive::pack(input, &rgx_path, options.level)?;
    let rgx_pack_time = started.elapsed();
    let rgx_archive_bytes = fs::metadata(&rgx_path)?.len();

    let rgx_output = workspace.path().join("rgx-extract");
    let started = Instant::now();
    archive::extract(&rgx_path, &rgx_output)?;
    let rgx_extract_time = started.elapsed();
    results.push(BenchmarkResult {
        name: "RGX".to_string(),
        archive_bytes: rgx_archive_bytes,
        pack_time: rgx_pack_time,
        extract_time: rgx_extract_time,
    });

    if options.include_private {
        let private_path = workspace.path().join("benchmark-private.rgx");
        let started = Instant::now();
        private::pack_private(
            input,
            &private_path,
            options.level,
            BENCHMARK_PRIVATE_PASSWORD,
        )?;
        let private_pack_time = started.elapsed();
        let private_archive_bytes = fs::metadata(&private_path)?.len();

        let private_output = workspace.path().join("private-extract");
        let started = Instant::now();
        private::extract_private(
            &private_path,
            &private_output,
            BENCHMARK_PRIVATE_PASSWORD,
        )?;
        let private_extract_time = started.elapsed();
        results.push(BenchmarkResult {
            name: "RGX Private".to_string(),
            archive_bytes: private_archive_bytes,
            pack_time: private_pack_time,
            extract_time: private_extract_time,
        });
    }

    let zip_path = workspace.path().join("benchmark.zip");
    let started = Instant::now();
    write_zip(input, &zip_path)?;
    let zip_pack_time = started.elapsed();
    let zip_archive_bytes = fs::metadata(&zip_path)?.len();

    let zip_output = workspace.path().join("zip-extract");
    let started = Instant::now();
    extract_zip(&zip_path, &zip_output)?;
    let zip_extract_time = started.elapsed();
    results.push(BenchmarkResult {
        name: "ZIP (Deflate)".to_string(),
        archive_bytes: zip_archive_bytes,
        pack_time: zip_pack_time,
        extract_time: zip_extract_time,
    });

    if options.include_7zip {
        match find_7zip() {
            Some(binary) => match benchmark_7zip(input, workspace.path(), binary) {
                Ok(result) => results.push(result),
                Err(error) => skipped.push(format!("7-Zip: {error}")),
            },
            None => skipped.push("7-Zip: executable not found (tried 7z, 7zz, 7za)".to_string()),
        }
    }

    Ok(BenchmarkReport {
        input_bytes,
        files,
        rgx_deduplicated_bytes: rgx_info.deduplicated_bytes,
        results,
        skipped,
    })
}

fn source_stats(input: &Path) -> Result<(u64, u64)> {
    if input.is_file() {
        return Ok((fs::metadata(input)?.len(), 1));
    }
    if !input.is_dir() {
        bail!("benchmark input must be a regular file or directory");
    }

    let mut bytes = 0u64;
    let mut files = 0u64;
    for item in WalkDir::new(input) {
        let item = item?;
        if item.file_type().is_symlink() {
            bail!(
                "symbolic links are not supported in the RGX benchmark: {}",
                item.path().display()
            );
        }
        if item.file_type().is_file() {
            bytes = bytes
                .checked_add(item.metadata()?.len())
                .ok_or_else(|| anyhow!("benchmark input size overflow"))?;
            files = files
                .checked_add(1)
                .ok_or_else(|| anyhow!("benchmark file count overflow"))?;
        }
    }
    Ok((bytes, files))
}

fn write_zip(input: &Path, output: &Path) -> Result<()> {
    let file = File::create(output)
        .with_context(|| format!("failed to create ZIP benchmark archive {}", output.display()))?;
    let writer = BufWriter::new(file);
    let mut zip = ZipWriter::new(writer);
    let file_options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let directory_options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let base = input.parent().unwrap_or_else(|| Path::new("."));

    let walker = if input.is_dir() {
        WalkDir::new(input)
    } else {
        WalkDir::new(input).max_depth(0)
    };

    for item in walker {
        let item = item?;
        if item.file_type().is_symlink() {
            bail!("symbolic links are not supported in ZIP benchmark input");
        }
        let relative = item.path().strip_prefix(base)?;
        let mut name = portable_path(relative)?;

        if item.file_type().is_dir() {
            if !name.ends_with('/') {
                name.push('/');
            }
            zip.add_directory(name, directory_options)?;
        } else if item.file_type().is_file() {
            zip.start_file(name, file_options)?;
            let mut source = BufReader::new(File::open(item.path())?);
            io::copy(&mut source, &mut zip)?;
        } else {
            bail!("unsupported filesystem entry in ZIP benchmark");
        }
    }

    let mut writer = zip.finish()?;
    use std::io::Write;
    writer.flush()?;
    Ok(())
}

fn extract_zip(archive_path: &Path, output: &Path) -> Result<()> {
    fs::create_dir(output)?;
    let file = File::open(archive_path)?;
    let reader = BufReader::new(file);
    let mut archive = ZipArchive::new(reader)?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| anyhow!("ZIP benchmark archive contains an unsafe path"))?;
        let target = output.join(enclosed);

        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut destination = File::create(&target)?;
        io::copy(&mut entry, &mut destination)?;
    }
    Ok(())
}

fn portable_path(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => parts.push(
                value
                    .to_str()
                    .ok_or_else(|| anyhow!("benchmark requires UTF-8 paths"))?,
            ),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("benchmark input contains an unsupported path")
            }
        }
    }
    if parts.is_empty() {
        bail!("benchmark input resolved to an empty archive path");
    }
    Ok(parts.join("/"))
}

fn find_7zip() -> Option<&'static str> {
    ["7z", "7zz", "7za"].into_iter().find(|candidate| {
        Command::new(candidate)
            .arg("i")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
    })
}

fn benchmark_7zip(input: &Path, workspace: &Path, binary: &str) -> Result<BenchmarkResult> {
    let base = input.parent().unwrap_or_else(|| Path::new("."));
    let name = input
        .file_name()
        .ok_or_else(|| anyhow!("7-Zip benchmark input must have a file name"))?;
    let archive_path = workspace.join("benchmark.7z");

    let started = Instant::now();
    let status = Command::new(binary)
        .current_dir(base)
        .arg("a")
        .arg("-t7z")
        .arg("-mx=5")
        .arg("-bd")
        .arg("-y")
        .arg(&archive_path)
        .arg(name)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to launch 7-Zip")?;
    if !status.success() {
        bail!("packing failed with status {status}");
    }
    let pack_time = started.elapsed();
    let archive_bytes = fs::metadata(&archive_path)?.len();

    let output = workspace.join("7zip-extract");
    fs::create_dir(&output)?;
    let output_argument = format!("-o{}", output.display());
    let started = Instant::now();
    let status = Command::new(binary)
        .arg("x")
        .arg(&archive_path)
        .arg(output_argument)
        .arg("-bd")
        .arg("-y")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to launch 7-Zip extraction")?;
    if !status.success() {
        bail!("extraction failed with status {status}");
    }
    let extract_time = started.elapsed();

    Ok(BenchmarkResult {
        name: "7-Zip (LZMA2 normal)".to_string(),
        archive_bytes,
        pack_time,
        extract_time,
    })
}

fn throughput(bytes: u64, duration: Duration) -> f64 {
    if bytes == 0 || duration.is_zero() {
        return 0.0;
    }
    bytes as f64 / (1024.0 * 1024.0) / duration.as_secs_f64()
}
