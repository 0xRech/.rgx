use std::io::{self, Read, Write};

pub const MAGIC: [u8; 4] = *b"RGX\0";
pub const ENTRY_MAGIC: [u8; 4] = *b"ENTR";
pub const FOOTER_MAGIC: [u8; 4] = *b"RGXF";
pub const VERSION_MAJOR: u16 = 0;
pub const VERSION_MINOR: u16 = 1;

pub const KIND_DIRECTORY: u8 = 0;
pub const KIND_FILE: u8 = 1;
pub const COMPRESSION_STORE: u8 = 0;
pub const COMPRESSION_ZSTD: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub flags: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryHeader {
    pub kind: u8,
    pub compression: u8,
    pub path_len: u32,
    pub original_size: u64,
    pub payload_size: u64,
    pub hash: [u8; 32],
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
    if major != VERSION_MAJOR || minor > VERSION_MINOR {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported RGX version {major}.{minor}"),
        ));
    }

    let flags = read_u32(reader)?;
    let _reserved = read_u32(reader)?;
    Ok(Header { flags })
}

pub fn write_entry_header<W: Write>(writer: &mut W, entry: &EntryHeader) -> io::Result<()> {
    writer.write_all(&ENTRY_MAGIC)?;
    writer.write_all(&[entry.kind, entry.compression])?;
    write_u16(writer, 0)?;
    write_u32(writer, entry.path_len)?;
    write_u64(writer, entry.original_size)?;
    write_u64(writer, entry.payload_size)?;
    writer.write_all(&entry.hash)?;
    Ok(())
}

pub fn read_entry_header_after_magic<R: Read>(reader: &mut R) -> io::Result<EntryHeader> {
    let mut types = [0u8; 2];
    reader.read_exact(&mut types)?;
    let _reserved = read_u16(reader)?;
    let path_len = read_u32(reader)?;
    let original_size = read_u64(reader)?;
    let payload_size = read_u64(reader)?;
    let mut hash = [0u8; 32];
    reader.read_exact(&mut hash)?;

    Ok(EntryHeader {
        kind: types[0],
        compression: types[1],
        path_len,
        original_size,
        payload_size,
        hash,
    })
}

pub fn write_footer<W: Write>(writer: &mut W, entry_count: u64) -> io::Result<()> {
    writer.write_all(&FOOTER_MAGIC)?;
    write_u64(writer, entry_count)
}

pub fn read_tag<R: Read>(reader: &mut R) -> io::Result<Option<[u8; 4]>> {
    let mut tag = [0u8; 4];
    match reader.read_exact(&mut tag) {
        Ok(()) => Ok(Some(tag)),
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
        Err(err) => Err(err),
    }
}

pub fn read_footer_count<R: Read>(reader: &mut R) -> io::Result<u64> {
    read_u64(reader)
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
