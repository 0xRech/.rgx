use crate::chunker;
use crate::format::{
    self, ChunkHeader, DirectoryHeader, FileHeader, Footer, Header, CHUNK_MAGIC,
    COMPRESSION_STORE, COMPRESSION_ZSTD, DIRECTORY_MAGIC, FILE_MAGIC, FOOTER_MAGIC,
    KIND_DIRECTORY, KIND_FILE,
};
use anyhow::{anyhow, bail, Context, Result};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;

const IO_BUFFER_SIZE: usize = 64 * 1024;
const MAX_PATH_BYTES: usize = 1024 * 1024;
const MAX_CHUNKS_PER_FILE: u32 = 4_194_304;

#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    pub path: String,
    pub kind: u8,
    pub original_size: u64,
    pub chunks: u32,
}

#[derive(Debug, Clone)]
pub struct ArchiveInfo {
    pub version: &'static str,
    pub entries: u64,
    pub files: u64,
    pub directories: u64,
    pub original_bytes: u64,
    pub stored_bytes: u64,
    pub unique_chunks: u64,
    pub chunk_references: u64,
    pub deduplicated_bytes: u64,
}

#[derive(Debug, Clone)]
struct ChunkMeta {
    compression: u8,
    original_size: u64,
    payload_size: u64,
    payload_offset: u64,
}

#[derive(Debug, Clone)]
struct FileRecord {
    path: String,
    original_size: u64,
    hash: [u8; 32],
    chunk_hashes: Vec<[u8; 32]>,
}

#[derive(Debug)]
struct ArchiveCatalog {
    chunks: HashMap<[u8; 32], ChunkMeta>,
    files: Vec<FileRecord>,
    directories: Vec<String>,
    info: ArchiveInfo,
}

#[derive(Debug, Default)]
struct PackStats {
    entries: u64,
    files: u64,
    directories: u64,
    original_bytes: u64,
    stored_bytes: u64,
    unique_chunks: u64,
    chunk_references: u64,
    deduplicated_bytes: u64,
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

    reject_output_inside_input(input, output)?;

    let output_file = File::create(output)
        .with_context(|| format!("failed to create {}", output.display()))?;
    let mut writer = BufWriter::new(output_file);
    format::write_header(&mut writer, &Header { flags: 0 })?;

    let base = input.parent().unwrap_or_else(|| Path::new("."));
    let walker = if input.is_dir() {
        WalkDir::new(input)
    } else {
        WalkDir::new(input).max_depth(0)
    };

    let mut stats = PackStats::default();
    let mut seen_chunks = HashSet::<[u8; 32]>::new();

    for item in walker {
        let item = item?;
        let path = item.path();
        if item.file_type().is_symlink() {
            bail!("symbolic links are not supported in RGX v0.2: {}", path.display());
        }

        let relative = path
            .strip_prefix(base)
            .with_context(|| format!("cannot relativize {}", path.display()))?;
        let stored_path = normalize_relative_path(relative)?;
        let path_bytes = stored_path.as_bytes();
        let path_len = checked_path_len(path_bytes)?;

        if item.file_type().is_dir() {
            format::write_directory_header(&mut writer, &DirectoryHeader { path_len })?;
            writer.write_all(path_bytes)?;
            stats.entries = checked_add(stats.entries, 1, "entry count")?;
            stats.directories = checked_add(stats.directories, 1, "directory count")?;
            continue;
        }

        if !item.file_type().is_file() {
            bail!("unsupported filesystem entry: {}", path.display());
        }

        pack_file(
            path,
            path_bytes,
            path_len,
            level,
            &mut writer,
            &mut seen_chunks,
            &mut stats,
        )?;
    }

    let footer = Footer {
        entries: stats.entries,
        files: stats.files,
        directories: stats.directories,
        unique_chunks: stats.unique_chunks,
        chunk_references: stats.chunk_references,
        original_bytes: stats.original_bytes,
        stored_payload_bytes: stats.stored_bytes,
        deduplicated_bytes: stats.deduplicated_bytes,
    };
    format::write_footer(&mut writer, &footer)?;
    writer.flush()?;

    Ok(info_from_footer(&footer))
}

pub fn list(archive: &Path) -> Result<Vec<ArchiveEntry>> {
    let catalog = scan_archive(archive)?;
    let mut entries = Vec::with_capacity(catalog.files.len() + catalog.directories.len());

    for path in catalog.directories {
        entries.push(ArchiveEntry {
            path,
            kind: KIND_DIRECTORY,
            original_size: 0,
            chunks: 0,
        });
    }
    for file in catalog.files {
        entries.push(ArchiveEntry {
            path: file.path,
            kind: KIND_FILE,
            original_size: file.original_size,
            chunks: u32::try_from(file.chunk_hashes.len()).context("too many chunk references")?,
        });
    }

    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
}

pub fn info(archive: &Path) -> Result<ArchiveInfo> {
    Ok(scan_archive(archive)?.info)
}

pub fn verify(archive: &Path) -> Result<ArchiveInfo> {
    let catalog = scan_archive(archive)?;
    let mut file = File::open(archive)
        .with_context(|| format!("failed to open {}", archive.display()))?;

    for (hash, meta) in &catalog.chunks {
        let _ = read_chunk_data(&mut file, hash, meta)?;
    }

    for record in &catalog.files {
        verify_file_record(&mut file, record, &catalog.chunks)?;
    }

    Ok(catalog.info)
}

pub fn extract(archive: &Path, output: &Path) -> Result<ArchiveInfo> {
    if output.exists() {
        bail!("output already exists: {}", output.display());
    }

    let catalog = scan_archive(archive)?;
    fs::create_dir(output)
        .with_context(|| format!("failed to create {}", output.display()))?;

    let mut directories = catalog.directories.clone();
    directories.sort_by_key(|path| path.matches('/').count());
    for path in directories {
        let safe_path = validate_archive_path(&path)?;
        fs::create_dir_all(output.join(safe_path))?;
    }

    let mut archive_file = File::open(archive)
        .with_context(|| format!("failed to open {}", archive.display()))?;
    for record in &catalog.files {
        extract_file_record(&mut archive_file, output, record, &catalog.chunks)?;
    }

    Ok(catalog.info)
}

fn pack_file<W: Write>(
    path: &Path,
    path_bytes: &[u8],
    path_len: u32,
    level: i32,
    writer: &mut W,
    seen_chunks: &mut HashSet<[u8; 32]>,
    stats: &mut PackStats,
) -> Result<()> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut reader = BufReader::with_capacity(IO_BUFFER_SIZE, file);
    let mut input_buffer = [0u8; IO_BUFFER_SIZE];
    let mut rolling = chunker::RollingHash::default();
    let mut chunk = Vec::with_capacity(chunker::MAX_CHUNK_SIZE);
    let mut file_hasher = blake3::Hasher::new();
    let mut file_size = 0u64;
    let mut chunk_hashes = Vec::<[u8; 32]>::new();

    loop {
        let read = reader.read(&mut input_buffer)?;
        if read == 0 {
            break;
        }

        let bytes = &input_buffer[..read];
        file_hasher.update(bytes);
        file_size = checked_add(file_size, read as u64, "file size")?;

        for &byte in bytes {
            chunk.push(byte);
            let state = rolling.push(byte);
            if chunker::should_cut(state, chunk.len()) {
                write_or_reference_chunk(
                    &chunk,
                    level,
                    writer,
                    seen_chunks,
                    &mut chunk_hashes,
                    stats,
                )?;
                chunk.clear();
            }
        }
    }

    if !chunk.is_empty() {
        write_or_reference_chunk(
            &chunk,
            level,
            writer,
            seen_chunks,
            &mut chunk_hashes,
            stats,
        )?;
    }

    let chunk_count = u32::try_from(chunk_hashes.len()).context("too many chunks in one file")?;
    if chunk_count > MAX_CHUNKS_PER_FILE {
        bail!("file contains too many chunks: {}", path.display());
    }

    let header = FileHeader {
        path_len,
        chunk_count,
        original_size: file_size,
        hash: *file_hasher.finalize().as_bytes(),
    };
    format::write_file_header(writer, &header)?;
    writer.write_all(path_bytes)?;
    for hash in &chunk_hashes {
        writer.write_all(hash)?;
    }

    stats.entries = checked_add(stats.entries, 1, "entry count")?;
    stats.files = checked_add(stats.files, 1, "file count")?;
    stats.original_bytes = checked_add(stats.original_bytes, file_size, "original byte count")?;
    Ok(())
}

fn write_or_reference_chunk<W: Write>(
    data: &[u8],
    level: i32,
    writer: &mut W,
    seen_chunks: &mut HashSet<[u8; 32]>,
    chunk_hashes: &mut Vec<[u8; 32]>,
    stats: &mut PackStats,
) -> Result<()> {
    if data.is_empty() {
        return Ok(());
    }

    let hash = *blake3::hash(data).as_bytes();
    chunk_hashes.push(hash);
    stats.chunk_references = checked_add(stats.chunk_references, 1, "chunk reference count")?;

    if seen_chunks.contains(&hash) {
        stats.deduplicated_bytes = checked_add(
            stats.deduplicated_bytes,
            data.len() as u64,
            "deduplicated byte count",
        )?;
        return Ok(());
    }

    let compressed = zstd::bulk::compress(data, level).context("zstd compression failed")?;
    let (compression, payload): (u8, &[u8]) = if compressed.len() < data.len() {
        (COMPRESSION_ZSTD, &compressed)
    } else {
        (COMPRESSION_STORE, data)
    };

    let header = ChunkHeader {
        compression,
        original_size: data.len() as u64,
        payload_size: payload.len() as u64,
        hash,
    };
    format::write_chunk_header(writer, &header)?;
    writer.write_all(payload)?;

    seen_chunks.insert(hash);
    stats.unique_chunks = checked_add(stats.unique_chunks, 1, "unique chunk count")?;
    stats.stored_bytes = checked_add(stats.stored_bytes, payload.len() as u64, "stored byte count")?;
    Ok(())
}

fn scan_archive(archive: &Path) -> Result<ArchiveCatalog> {
    let file = File::open(archive).with_context(|| format!("failed to open {}", archive.display()))?;
    let mut reader = BufReader::with_capacity(IO_BUFFER_SIZE, file);
    let _header = format::read_header(&mut reader)?;

    let mut chunks = HashMap::<[u8; 32], ChunkMeta>::new();
    let mut files = Vec::<FileRecord>::new();
    let mut directories = Vec::<String>::new();
    let mut paths = HashSet::<String>::new();
    let mut file_paths = HashSet::<String>::new();

    let mut entries = 0u64;
    let mut file_count = 0u64;
    let mut directory_count = 0u64;
    let mut chunk_references = 0u64;
    let mut original_bytes = 0u64;
    let mut stored_payload_bytes = 0u64;
    let mut referenced_logical_bytes = 0u64;

    let footer = loop {
        let tag = format::read_tag(&mut reader)?
            .ok_or_else(|| anyhow!("archive is missing its footer"))?;

        if tag == CHUNK_MAGIC {
            let header = format::read_chunk_header_after_magic(&mut reader)?;
            validate_chunk_header(&header)?;
            if chunks.contains_key(&header.hash) {
                bail!("duplicate chunk record in archive");
            }

            let payload_offset = reader.stream_position()?;
            let skip = i64::try_from(header.payload_size).context("chunk payload is too large")?;
            reader.seek(SeekFrom::Current(skip))?;

            stored_payload_bytes = checked_add(
                stored_payload_bytes,
                header.payload_size,
                "stored payload byte count",
            )?;
            chunks.insert(
                header.hash,
                ChunkMeta {
                    compression: header.compression,
                    original_size: header.original_size,
                    payload_size: header.payload_size,
                    payload_offset,
                },
            );
            continue;
        }

        if tag == DIRECTORY_MAGIC {
            let header = format::read_directory_header_after_magic(&mut reader)?;
            let path = read_path(&mut reader, header.path_len)?;
            validate_archive_path(&path)?;
            if !paths.insert(path.clone()) {
                bail!("duplicate archive path: {path}");
            }

            directories.push(path);
            entries = checked_add(entries, 1, "entry count")?;
            directory_count = checked_add(directory_count, 1, "directory count")?;
            continue;
        }

        if tag == FILE_MAGIC {
            let header = format::read_file_header_after_magic(&mut reader)?;
            if header.chunk_count > MAX_CHUNKS_PER_FILE {
                bail!("file declares too many chunks");
            }
            let path = read_path(&mut reader, header.path_len)?;
            validate_archive_path(&path)?;
            if !paths.insert(path.clone()) {
                bail!("duplicate archive path: {path}");
            }
            file_paths.insert(path.clone());

            let mut chunk_hashes = Vec::with_capacity(header.chunk_count as usize);
            let mut reconstructed_size = 0u64;
            for _ in 0..header.chunk_count {
                let mut hash = [0u8; 32];
                reader.read_exact(&mut hash)?;
                let meta = chunks
                    .get(&hash)
                    .ok_or_else(|| anyhow!("file {path} references an unknown or forward chunk"))?;
                reconstructed_size = checked_add(
                    reconstructed_size,
                    meta.original_size,
                    "file reconstructed size",
                )?;
                referenced_logical_bytes = checked_add(
                    referenced_logical_bytes,
                    meta.original_size,
                    "referenced logical byte count",
                )?;
                chunk_references = checked_add(chunk_references, 1, "chunk reference count")?;
                chunk_hashes.push(hash);
            }

            if reconstructed_size != header.original_size {
                bail!("chunk sizes do not reconstruct the declared size of {path}");
            }

            original_bytes = checked_add(original_bytes, header.original_size, "original byte count")?;
            files.push(FileRecord {
                path,
                original_size: header.original_size,
                hash: header.hash,
                chunk_hashes,
            });
            entries = checked_add(entries, 1, "entry count")?;
            file_count = checked_add(file_count, 1, "file count")?;
            continue;
        }

        if tag == FOOTER_MAGIC {
            break format::read_footer_after_magic(&mut reader)?;
        }

        bail!("unknown RGX record marker");
    };

    let mut trailing = [0u8; 1];
    if reader.read(&mut trailing)? != 0 {
        bail!("archive contains trailing data after footer");
    }

    validate_path_tree(&paths, &file_paths)?;

    let unique_logical_bytes = chunks.values().try_fold(0u64, |acc, meta| {
        checked_add(acc, meta.original_size, "unique logical byte count")
    })?;
    if referenced_logical_bytes < unique_logical_bytes {
        bail!("archive contains unreferenced chunk data");
    }
    let deduplicated_bytes = referenced_logical_bytes - unique_logical_bytes;

    let expected = Footer {
        entries,
        files: file_count,
        directories: directory_count,
        unique_chunks: chunks.len() as u64,
        chunk_references,
        original_bytes,
        stored_payload_bytes,
        deduplicated_bytes,
    };
    if footer != expected {
        bail!("RGX footer statistics do not match archive contents");
    }

    Ok(ArchiveCatalog {
        chunks,
        files,
        directories,
        info: info_from_footer(&footer),
    })
}

fn verify_file_record(
    archive: &mut File,
    record: &FileRecord,
    chunks: &HashMap<[u8; 32], ChunkMeta>,
) -> Result<()> {
    let mut hasher = blake3::Hasher::new();
    let mut size = 0u64;

    for hash in &record.chunk_hashes {
        let meta = chunks
            .get(hash)
            .ok_or_else(|| anyhow!("missing chunk while verifying {}", record.path))?;
        let data = read_chunk_data(archive, hash, meta)?;
        hasher.update(&data);
        size = checked_add(size, data.len() as u64, "verified file size")?;
    }

    if size != record.original_size {
        bail!("size verification failed for {}", record.path);
    }
    if hasher.finalize().as_bytes() != &record.hash {
        bail!("BLAKE3 file verification failed for {}", record.path);
    }
    Ok(())
}

fn extract_file_record(
    archive: &mut File,
    root: &Path,
    record: &FileRecord,
    chunks: &HashMap<[u8; 32], ChunkMeta>,
) -> Result<()> {
    let safe_path = validate_archive_path(&record.path)?;
    let target = root.join(safe_path);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .with_context(|| format!("failed to create {}", target.display()))?;

    let result = (|| -> Result<()> {
        let mut hasher = blake3::Hasher::new();
        let mut size = 0u64;

        for hash in &record.chunk_hashes {
            let meta = chunks
                .get(hash)
                .ok_or_else(|| anyhow!("missing chunk while extracting {}", record.path))?;
            let data = read_chunk_data(archive, hash, meta)?;
            output.write_all(&data)?;
            hasher.update(&data);
            size = checked_add(size, data.len() as u64, "extracted file size")?;
        }
        output.flush()?;

        if size != record.original_size {
            bail!("size verification failed for {}", record.path);
        }
        if hasher.finalize().as_bytes() != &record.hash {
            bail!("BLAKE3 file verification failed for {}", record.path);
        }
        Ok(())
    })();

    if result.is_err() {
        drop(output);
        let _ = fs::remove_file(&target);
    }
    result
}

fn read_chunk_data(archive: &mut File, expected_hash: &[u8; 32], meta: &ChunkMeta) -> Result<Vec<u8>> {
    archive.seek(SeekFrom::Start(meta.payload_offset))?;
    let payload_len = usize::try_from(meta.payload_size).context("chunk payload is too large")?;
    let mut payload = vec![0u8; payload_len];
    archive.read_exact(&mut payload)?;

    let data = match meta.compression {
        COMPRESSION_STORE => payload,
        COMPRESSION_ZSTD => zstd::bulk::decompress(
            &payload,
            usize::try_from(meta.original_size).context("chunk is too large")?,
        )
        .context("zstd decompression failed")?,
        other => bail!("unsupported compression method {other}"),
    };

    if data.len() as u64 != meta.original_size {
        bail!("chunk size verification failed");
    }
    if blake3::hash(&data).as_bytes() != expected_hash {
        bail!("BLAKE3 chunk verification failed");
    }
    Ok(data)
}

fn validate_chunk_header(header: &ChunkHeader) -> Result<()> {
    if header.original_size == 0 || header.original_size > chunker::MAX_CHUNK_SIZE as u64 {
        bail!("invalid RGX chunk size");
    }
    match header.compression {
        COMPRESSION_STORE => {
            if header.payload_size != header.original_size {
                bail!("stored chunk payload size does not match original size");
            }
        }
        COMPRESSION_ZSTD => {
            if header.payload_size == 0 || header.payload_size >= header.original_size {
                bail!("invalid compressed chunk size");
            }
        }
        other => bail!("unsupported compression method {other}"),
    }
    Ok(())
}

fn read_path<R: Read>(reader: &mut R, path_len: u32) -> Result<String> {
    let path_len = path_len as usize;
    if path_len == 0 || path_len > MAX_PATH_BYTES {
        bail!("invalid RGX path length");
    }
    let mut bytes = vec![0u8; path_len];
    reader.read_exact(&mut bytes)?;
    String::from_utf8(bytes).context("RGX path is not valid UTF-8")
}

fn checked_path_len(path: &[u8]) -> Result<u32> {
    if path.is_empty() || path.len() > MAX_PATH_BYTES {
        bail!("invalid RGX path length");
    }
    u32::try_from(path.len()).context("path is too long")
}

fn normalize_relative_path(path: &Path) -> Result<String> {
    validate_components(path)?;
    let mut parts = Vec::new();
    for component in path.components() {
        if let Component::Normal(value) = component {
            parts.push(
                value
                    .to_str()
                    .ok_or_else(|| anyhow!("RGX v0.2 requires UTF-8 paths"))?,
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
    if candidate.components().next().is_none() {
        bail!("archive path is empty");
    }
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

fn validate_path_tree(paths: &HashSet<String>, file_paths: &HashSet<String>) -> Result<()> {
    for path in paths {
        let mut current = Path::new(path).parent();
        while let Some(parent) = current {
            if parent.as_os_str().is_empty() {
                break;
            }
            let parent = normalize_relative_path(parent)?;
            if file_paths.contains(&parent) {
                bail!("file path {parent} is used as a parent directory");
            }
            current = Path::new(&parent).parent();
        }
    }
    Ok(())
}

fn reject_output_inside_input(input: &Path, output: &Path) -> Result<()> {
    if !input.is_dir() {
        return Ok(());
    }

    let input = fs::canonicalize(input)
        .with_context(|| format!("failed to canonicalize {}", input.display()))?;
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let parent = fs::canonicalize(parent)
        .with_context(|| format!("failed to canonicalize output directory {}", parent.display()))?;
    let file_name = output
        .file_name()
        .ok_or_else(|| anyhow!("output path must include a file name"))?;
    let output = parent.join(file_name);

    if output.starts_with(&input) {
        bail!("output archive must not be created inside the directory being packed");
    }
    Ok(())
}

fn checked_add(left: u64, right: u64, label: &str) -> Result<u64> {
    left.checked_add(right)
        .ok_or_else(|| anyhow!("{label} overflow"))
}

fn info_from_footer(footer: &Footer) -> ArchiveInfo {
    ArchiveInfo {
        version: "0.2",
        entries: footer.entries,
        files: footer.files,
        directories: footer.directories,
        original_bytes: footer.original_bytes,
        stored_bytes: footer.stored_payload_bytes,
        unique_chunks: footer.unique_chunks,
        chunk_references: footer.chunk_references,
        deduplicated_bytes: footer.deduplicated_bytes,
    }
}
