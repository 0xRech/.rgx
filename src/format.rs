use std::io::{self, Read, Write};

pub const MAGIC: [u8; 4] = *b"RGX\0";
pub const CHUNK_MAGIC: [u8; 4] = *b"CHNK";
pub const FILE_MAGIC: [u8; 4] = *b"FILE";
pub const DIRECTORY_MAGIC: [u8; 4] = *b"DIRE";
pub const FOOTER_MAGIC: [u8; 4] = *b"RGXF";

pub const VERSION_MAJOR: u16 = 0;
pub const VERSION_MINOR: u16 = 2;

pub const KIND_DIRECTORY: u8 = 0;
pub const KIND_FILE: u8 = 1;

pub const COMPRESSION_STORE: u8 = 0;
pub const COMPRESSION_ZSTD: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub flags: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkHeader {
    pub compression: u8,
    pub original_size: u64,
    pub payload_size: u64,
    pub hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHeader {
    pub path_len: u32,
    pub chunk_count: u32,
    pub original_size: u64,
    pub hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryHeader {
    pub path_len: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footer {
    pub entries: u64,
    pub files: u64,
    pub directories: u64,
    pub unique_chunks: u64,
    pub chunk_references: u64,
    pub original_bytes: u64,
    pub stored_payload_bytes: u64,
    pub deduplicated_bytes: u64,
}

pub fn write_header<W: Write>(writer: &mut W, header: &Header) -> io::Result<()> {
    writer.write_all(&MAGIC)?;
    write_u16(writer, VERSION_MAJOR)?;
    write_u16(writer, VERSION_MINOR)?;
    write_u32(writer, header.flags)?;
    write_u32(writer, 0)?;
    Ok(())
}

pub fn read_header<R: Read>(reader: &mut R) -> io::Result<Header> {
    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic)?;
    if magic != MAGIC {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not an RGX archive"));
    }

    let major = read_u16(reader)?;
    let minor = read_u16(reader)?;
    if major != VERSION_MAJOR || minor != VERSION_MINOR {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported RGX version {major}.{minor}; this build expects 0.2"),
        ));
    }

    let flags = read_u32(reader)?;
    let _reserved = read_u32(reader)?;
    Ok(Header { flags })
}

pub fn write_chunk_header<W: Write>(writer: &mut W, chunk: &ChunkHeader) -> io::Result<()> {
    writer.write_all(&CHUNK_MAGIC)?;
    writer.write_all(&[chunk.compression])?;
    writer.write_all(&[0u8; 3])?;
    write_u64(writer, chunk.original_size)?;
    write_u64(writer, chunk.payload_size)?;
    writer.write_all(&chunk.hash)?;
    Ok(())
}

pub fn read_chunk_header_after_magic<R: Read>(reader: &mut R) -> io::Result<ChunkHeader> {
    let mut compression = [0u8; 1];
    reader.read_exact(&mut compression)?;
    let mut reserved = [0u8; 3];
    reader.read_exact(&mut reserved)?;
    let original_size = read_u64(reader)?;
    let payload_size = read_u64(reader)?;
    let mut hash = [0u8; 32];
    reader.read_exact(&mut hash)?;

    Ok(ChunkHeader {
        compression: compression[0],
        original_size,
        payload_size,
        hash,
    })
}

pub fn write_file_header<W: Write>(writer: &mut W, file: &FileHeader) -> io::Result<()> {
    writer.write_all(&FILE_MAGIC)?;
    write_u32(writer, file.path_len)?;
    write_u32(writer, file.chunk_count)?;
    write_u64(writer, file.original_size)?;
    writer.write_all(&file.hash)?;
    Ok(())
}

pub fn read_file_header_after_magic<R: Read>(reader: &mut R) -> io::Result<FileHeader> {
    let path_len = read_u32(reader)?;
    let chunk_count = read_u32(reader)?;
    let original_size = read_u64(reader)?;
    let mut hash = [0u8; 32];
    reader.read_exact(&mut hash)?;

    Ok(FileHeader {
        path_len,
        chunk_count,
        original_size,
        hash,
    })
}

pub fn write_directory_header<W: Write>(
    writer: &mut W,
    directory: &DirectoryHeader,
) -> io::Result<()> {
    writer.write_all(&DIRECTORY_MAGIC)?;
    write_u32(writer, directory.path_len)?;
    write_u32(writer, 0)?;
    Ok(())
}

pub fn read_directory_header_after_magic<R: Read>(reader: &mut R) -> io::Result<DirectoryHeader> {
    let path_len = read_u32(reader)?;
    let _reserved = read_u32(reader)?;
    Ok(DirectoryHeader { path_len })
}

pub fn write_footer<W: Write>(writer: &mut W, footer: &Footer) -> io::Result<()> {
    writer.write_all(&FOOTER_MAGIC)?;
    write_u64(writer, footer.entries)?;
    write_u64(writer, footer.files)?;
    write_u64(writer, footer.directories)?;
    write_u64(writer, footer.unique_chunks)?;
    write_u64(writer, footer.chunk_references)?;
    write_u64(writer, footer.original_bytes)?;
    write_u64(writer, footer.stored_payload_bytes)?;
    write_u64(writer, footer.deduplicated_bytes)?;
    Ok(())
}

pub fn read_footer_after_magic<R: Read>(reader: &mut R) -> io::Result<Footer> {
    Ok(Footer {
        entries: read_u64(reader)?,
        files: read_u64(reader)?,
        directories: read_u64(reader)?,
        unique_chunks: read_u64(reader)?,
        chunk_references: read_u64(reader)?,
        original_bytes: read_u64(reader)?,
        stored_payload_bytes: read_u64(reader)?,
        deduplicated_bytes: read_u64(reader)?,
    })
}

pub fn read_tag<R: Read>(reader: &mut R) -> io::Result<Option<[u8; 4]>> {
    let mut tag = [0u8; 4];
    match reader.read_exact(&mut tag) {
        Ok(()) => Ok(Some(tag)),
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
        Err(err) => Err(err),
    }
}

fn write_u16<W: Write>(writer: &mut W, value: u16) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}

fn write_u32<W: Write>(writer: &mut W, value: u32) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}

fn write_u64<W: Write>(writer: &mut W, value: u64) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}

fn read_u16<R: Read>(reader: &mut R) -> io::Result<u16> {
    let mut bytes = [0u8; 2];
    reader.read_exact(&mut bytes)?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_u32<R: Read>(reader: &mut R) -> io::Result<u32> {
    let mut bytes = [0u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64<R: Read>(reader: &mut R) -> io::Result<u64> {
    let mut bytes = [0u8; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}
