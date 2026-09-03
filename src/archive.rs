use crate::format::{
    self, EntryHeader, Header, COMPRESSION_STORE, COMPRESSION_ZSTD, ENTRY_MAGIC, FOOTER_MAGIC,
    KIND_DIRECTORY, KIND_FILE,
};
use anyhow::{anyhow, bail, Context, Result};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    pub path: String,
    pub kind: u8,
    pub compression: u8,
    pub original_size: u64,
    pub payload_size: u64,
    pub hash: [u8; 32],
}

#[derive(Debug, Clone)]
pub struct ArchiveInfo {
    pub version: &'static str,
    pub entries: u64,
    pub files: u64,
    pub directories: u64,
    pub original_bytes: u64,
    pub stored_bytes: u64,
}

pub fn pack(input: &Path, output: &Path, level: i32) -> Result<ArchiveInfo> {
    if !input.exists() {
        bail!("input does not exist: {}", input.display());
    }
    if output.exists() {
        bail!("output already exists: {}", output.display());
    }
    if !(1..=22).contains(&level) {
        bail!("zstd level must be between 1 and 22");
    }

    let output_file = File::create(output)
        .with_context(|| format!("failed to create {}", output.display()))?;
    let mut writer = BufWriter::new(output_file);
    format::write_header(&mut writer, &Header { flags: 0 })?;

    let base = input.parent().unwrap_or_else(|| Path::new("."));

    let mut entries = 0u64;
    let mut files = 0u64;
    let mut directories = 0u64;
    let mut original_bytes = 0u64;
    let mut stored_bytes = 0u64;

    let walker = if input.is_dir() {
        WalkDir::new(input)
    } else {
        WalkDir::new(input).max_depth(0)
    };

    for item in walker {
        let item = item?;
        let path = item.path();
        if item.file_type().is_symlink() {
            bail!("symbolic links are not supported in RGX v0.1: {}", path.display());
        }

        let relative = path
            .strip_prefix(base)
            .with_context(|| format!("cannot relativize {}", path.display()))?;
        let stored_path = normalize_relative_path(relative)?;
        let path_bytes = stored_path.as_bytes();
        let path_len = u32::try_from(path_bytes.len()).context("path is too long")?;

        if item.file_type().is_dir() {
            let header = EntryHeader {
                kind: KIND_DIRECTORY,
                compression: COMPRESSION_STORE,
                path_len,
                original_size: 0,
                payload_size: 0,
                hash: [0u8; 32],
            };
            format::write_entry_header(&mut writer, &header)?;
            writer.write_all(path_bytes)?;
            entries += 1;
            directories += 1;
            continue;
        }

        if !item.file_type().is_file() {
            bail!("unsupported filesystem entry: {}", path.display());
        }

        let data = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
        let hash = *blake3::hash(&data).as_bytes();
        let compressed = zstd::bulk::compress(&data, level).context("zstd compression failed")?;
        let (compression, payload): (u8, &[u8]) = if compressed.len() < data.len() {
            (COMPRESSION_ZSTD, &compressed)
        } else {
            (COMPRESSION_STORE, &data)
        };

        let original_size = u64::try_from(data.len()).context("file too large")?;
        let payload_size = u64::try_from(payload.len()).context("payload too large")?;
        let header = EntryHeader {
            kind: KIND_FILE,
            compression,
            path_len,
            original_size,
            payload_size,
            hash,
        };

        format::write_entry_header(&mut writer, &header)?;
        writer.write_all(path_bytes)?;
        writer.write_all(payload)?;

        entries += 1;
        files += 1;
        original_bytes += original_size;
        stored_bytes += payload_size;
    }

    format::write_footer(&mut writer, entries)?;
    writer.flush()?;

    Ok(ArchiveInfo {
        version: "0.1",
        entries,
        files,
        directories,
        original_bytes,
        stored_bytes,
    })
}

pub fn list(archive: &Path) -> Result<Vec<ArchiveEntry>> {
    scan_archive(archive, false, None)
}

pub fn verify(archive: &Path) -> Result<ArchiveInfo> {
    let mut entries_out = Vec::new();
    let info = scan_archive_internal(archive, true, None, &mut entries_out)?;
    Ok(info)
}

pub fn info(archive: &Path) -> Result<ArchiveInfo> {
    let mut entries_out = Vec::new();
    scan_archive_internal(archive, false, None, &mut entries_out)
}

pub fn extract(archive: &Path, output: &Path) -> Result<ArchiveInfo> {
    if output.exists() {
        bail!("output already exists: {}", output.display());
    }
    fs::create_dir_all(output)
        .with_context(|| format!("failed to create {}", output.display()))?;
    let mut entries_out = Vec::new();
    scan_archive_internal(archive, true, Some(output), &mut entries_out)
}

fn scan_archive(archive: &Path, verify_hashes: bool, output: Option<&Path>) -> Result<Vec<ArchiveEntry>> {
    let mut entries = Vec::new();
    scan_archive_internal(archive, verify_hashes, output, &mut entries)?;
    Ok(entries)
}

fn scan_archive_internal(
    archive: &Path,
    verify_hashes: bool,
    output: Option<&Path>,
    entries_out: &mut Vec<ArchiveEntry>,
) -> Result<ArchiveInfo> {
    let file = File::open(archive).with_context(|| format!("failed to open {}", archive.display()))?;
    let mut reader = BufReader::new(file);
    let _header = format::read_header(&mut reader)?;

    let mut entries = 0u64;
    let mut files = 0u64;
    let mut directories = 0u64;
    let mut original_bytes = 0u64;
    let mut stored_bytes = 0u64;

    loop {
        let tag = format::read_tag(&mut reader)?.ok_or_else(|| anyhow!("archive is missing its footer"))?;
        if tag == FOOTER_MAGIC {
            let declared_entries = format::read_footer_count(&mut reader)?;
            if declared_entries != entries {
                bail!(
                    "archive footer declares {declared_entries} entries but {entries} were read"
                );
            }
            break;
        }
        if tag != ENTRY_MAGIC {
            bail!("invalid RGX entry marker");
        }

        let entry = format::read_entry_header_after_magic(&mut reader)?;
        if entry.path_len == 0 {
            bail!("RGX entry contains an empty path");
        }
        let mut path_bytes = vec![0u8; entry.path_len as usize];
        reader.read_exact(&mut path_bytes)?;
        let path = String::from_utf8(path_bytes).context("RGX entry path is not valid UTF-8")?;
        let safe_path = validate_archive_path(&path)?;

        match entry.kind {
            KIND_DIRECTORY => {
                if entry.original_size != 0 || entry.payload_size != 0 {
                    bail!("directory entry contains payload data: {path}");
                }
                if let Some(root) = output {
                    fs::create_dir_all(root.join(&safe_path))?;
                }
                directories += 1;
            }
            KIND_FILE => {
                let payload_len = usize::try_from(entry.payload_size).context("payload is too large")?;
                let mut payload = vec![0u8; payload_len];
                reader.read_exact(&mut payload)?;

                let data = match entry.compression {
                    COMPRESSION_STORE => payload,
                    COMPRESSION_ZSTD => zstd::bulk::decompress(
                        &payload,
                        usize::try_from(entry.original_size).context("file is too large")?,
                    )
                    .context("zstd decompression failed")?,
                    other => bail!("unsupported compression method {other} for {path}"),
                };

                if data.len() as u64 != entry.original_size {
                    bail!("size mismatch for {path}");
                }

                if verify_hashes || output.is_some() {
                    let actual = blake3::hash(&data);
                    if actual.as_bytes() != &entry.hash {
                        bail!("BLAKE3 verification failed for {path}");
                    }
                }

                if let Some(root) = output {
                    let target = root.join(&safe_path);
                    if let Some(parent) = target.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    if target.exists() {
                        bail!("refusing to overwrite existing path: {}", target.display());
                    }
                    fs::write(&target, &data)
                        .with_context(|| format!("failed to write {}", target.display()))?;
                }

                files += 1;
                original_bytes += entry.original_size;
                stored_bytes += entry.payload_size;
            }
            other => bail!("unsupported RGX entry kind {other}"),
        }

        entries += 1;
        entries_out.push(ArchiveEntry {
            path,
            kind: entry.kind,
            compression: entry.compression,
            original_size: entry.original_size,
            payload_size: entry.payload_size,
            hash: entry.hash,
        });
    }

    Ok(ArchiveInfo {
        version: "0.1",
        entries,
        files,
        directories,
        original_bytes,
        stored_bytes,
    })
}

fn normalize_relative_path(path: &Path) -> Result<String> {
    validate_components(path)?;
    let mut parts = Vec::new();
    for component in path.components() {
        if let Component::Normal(value) = component {
            parts.push(
                value
                    .to_str()
                    .ok_or_else(|| anyhow!("RGX v0.1 requires UTF-8 paths"))?,
            );
        }
    }
    if parts.is_empty() {
        bail!("cannot store an empty path");
    }
    Ok(parts.join("/"))
}

fn validate_archive_path(path: &str) -> Result<PathBuf> {
    if path.contains('\\') {
        bail!("archive path contains a backslash: {path}");
    }
    let candidate = Path::new(path);
    validate_components(candidate)?;
    Ok(candidate.to_path_buf())
}

fn validate_components(path: &Path) -> Result<()> {
    if path.is_absolute() {
        bail!("absolute paths are not allowed in RGX archives");
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => bail!("current-directory components are not allowed"),
            Component::ParentDir => bail!("parent-directory components are not allowed"),
            Component::RootDir | Component::Prefix(_) => bail!("absolute paths are not allowed"),
        }
    }
    Ok(())
}
