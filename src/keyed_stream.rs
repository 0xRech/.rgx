use crate::recipient::ArchiveKey;
use anyhow::{anyhow, bail, Context, Result};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use rand_core::{OsRng, RngCore};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub const KEYED_MAGIC: [u8; 4] = *b"RGXK";
const VERSION: u16 = 1;
const HEADER_SIZE: usize = 64;
const FRAME_MAGIC: [u8; 4] = *b"KFRM";
const FRAME_HEADER_SIZE: usize = 24;
const TAG_SIZE: usize = 16;
const NONCE_PREFIX_SIZE: usize = 16;
const DEFAULT_FRAME_SIZE: u32 = 1024 * 1024;

#[derive(Debug, Clone)]
struct StreamHeader {
    frame_size: u32,
    nonce_prefix: [u8; NONCE_PREFIX_SIZE],
    envelope_hash: [u8; 32],
}

pub struct KeyedWriter<W: Write> {
    writer: W,
    header: StreamHeader,
    header_bytes: [u8; HEADER_SIZE],
    cipher: XChaCha20Poly1305,
    buffer: Vec<u8>,
    sequence: u64,
    finished: bool,
}

impl<W: Write> KeyedWriter<W> {
    pub fn new(
        mut writer: W,
        archive_key: &ArchiveKey,
        envelope_hash: [u8; 32],
    ) -> Result<Self> {
        let mut nonce_prefix = [0u8; NONCE_PREFIX_SIZE];
        OsRng.fill_bytes(&mut nonce_prefix);
        let header = StreamHeader {
            frame_size: DEFAULT_FRAME_SIZE,
            nonce_prefix,
            envelope_hash,
        };
        let header_bytes = encode_header(&header);
        writer.write_all(&header_bytes)?;
        let cipher = XChaCha20Poly1305::new_from_slice(archive_key)
            .map_err(|_| anyhow!("failed to initialize RGX keyed payload cipher"))?;
        Ok(Self {
            writer,
            header,
            header_bytes,
            cipher,
            buffer: Vec::with_capacity(DEFAULT_FRAME_SIZE as usize),
            sequence: 0,
            finished: false,
        })
    }

    fn emit(&mut self, last: bool) -> Result<()> {
        let plaintext_len =
            u32::try_from(self.buffer.len()).context("RGX keyed payload frame is too large")?;
        if plaintext_len > self.header.frame_size {
            bail!("RGX keyed payload frame exceeds configured frame size");
        }
        let ciphertext_len = plaintext_len
            .checked_add(TAG_SIZE as u32)
            .ok_or_else(|| anyhow!("RGX keyed payload ciphertext length overflow"))?;
        let frame_header = encode_frame_header(self.sequence, plaintext_len, ciphertext_len, last);
        let nonce = frame_nonce(&self.header.nonce_prefix, self.sequence);
        let aad = frame_aad(&self.header_bytes, &frame_header);
        let ciphertext = self
            .cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &self.buffer,
                    aad: &aad,
                },
            )
            .map_err(|_| anyhow!("RGX keyed payload encryption failed"))?;
        self.writer.write_all(&frame_header)?;
        self.writer.write_all(&ciphertext)?;
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| anyhow!("RGX keyed payload frame sequence overflow"))?;
        self.buffer.clear();
        Ok(())
    }

    pub fn finish(&mut self) -> Result<()> {
        if !self.finished {
            self.emit(true)?;
            self.writer.flush()?;
            self.finished = true;
        }
        Ok(())
    }
}

impl<W: Write> Write for KeyedWriter<W> {
    fn write(&mut self, mut data: &[u8]) -> io::Result<usize> {
        if self.finished {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "RGX keyed payload writer is finished",
            ));
        }
        let original = data.len();
        while !data.is_empty() {
            let available = self.header.frame_size as usize - self.buffer.len();
            let take = available.min(data.len());
            self.buffer.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.buffer.len() == self.header.frame_size as usize {
                self.emit(false).map_err(to_io_error)?;
            }
        }
        Ok(original)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

pub struct KeyedReader {
    file: File,
    stream_offset: u64,
    header: StreamHeader,
    header_bytes: [u8; HEADER_SIZE],
    cipher: XChaCha20Poly1305,
    position: u64,
    plaintext_len: u64,
    final_sequence: u64,
    cached_sequence: Option<u64>,
    cached_plaintext: Vec<u8>,
}

impl KeyedReader {
    pub fn open(
        path: &Path,
        stream_offset: u64,
        archive_key: &ArchiveKey,
        expected_envelope_hash: [u8; 32],
    ) -> Result<Self> {
        let mut file =
            File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
        file.seek(SeekFrom::Start(stream_offset))?;
        let mut header_bytes = [0u8; HEADER_SIZE];
        file.read_exact(&mut header_bytes)
            .context("RGX keyed payload header is truncated")?;
        let header = decode_header(&header_bytes)?;
        if header.envelope_hash != expected_envelope_hash {
            bail!("RGX recipient envelope authentication binding does not match the payload");
        }
        let cipher = XChaCha20Poly1305::new_from_slice(archive_key)
            .map_err(|_| anyhow!("failed to initialize RGX keyed payload cipher"))?;

        let file_len = file.metadata()?.len();
        let stride = FRAME_HEADER_SIZE as u64 + header.frame_size as u64 + TAG_SIZE as u64;
        let frames_start = stream_offset
            .checked_add(HEADER_SIZE as u64)
            .ok_or_else(|| anyhow!("RGX keyed payload offset overflow"))?;
        let mut sequence = 0u64;
        let (plaintext_len, final_sequence) = loop {
            let offset = frames_start
                .checked_add(
                    sequence
                        .checked_mul(stride)
                        .ok_or_else(|| anyhow!("RGX keyed payload frame offset overflow"))?,
                )
                .ok_or_else(|| anyhow!("RGX keyed payload frame offset overflow"))?;
            if offset + FRAME_HEADER_SIZE as u64 > file_len {
                bail!("RGX keyed payload ended before its final frame");
            }
            file.seek(SeekFrom::Start(offset))?;
            let mut frame = [0u8; FRAME_HEADER_SIZE];
            file.read_exact(&mut frame)?;
            let (actual, plain, cipher_len, last) = decode_frame_header(&frame, &header)?;
            if actual != sequence {
                bail!("RGX keyed payload frame sequence mismatch");
            }
            let end = offset
                .checked_add(FRAME_HEADER_SIZE as u64)
                .and_then(|value| value.checked_add(cipher_len as u64))
                .ok_or_else(|| anyhow!("RGX keyed payload frame length overflow"))?;
            if end > file_len {
                bail!("RGX keyed payload frame is truncated");
            }
            if last {
                if end != file_len {
                    bail!("RGX keyed payload contains trailing data after its final frame");
                }
                let total = sequence
                    .checked_mul(header.frame_size as u64)
                    .and_then(|value| value.checked_add(plain as u64))
                    .ok_or_else(|| anyhow!("RGX keyed payload plaintext length overflow"))?;
                break (total, sequence);
            }
            sequence = sequence
                .checked_add(1)
                .ok_or_else(|| anyhow!("RGX keyed payload frame sequence overflow"))?;
        };

        Ok(Self {
            file,
            stream_offset,
            header,
            header_bytes,
            cipher,
            position: 0,
            plaintext_len,
            final_sequence,
            cached_sequence: None,
            cached_plaintext: Vec::new(),
        })
    }

    fn load_frame(&mut self, sequence: u64) -> Result<()> {
        if self.cached_sequence == Some(sequence) {
            return Ok(());
        }
        if sequence > self.final_sequence {
            bail!("RGX keyed payload seek is outside the plaintext stream");
        }
        let stride = FRAME_HEADER_SIZE as u64 + self.header.frame_size as u64 + TAG_SIZE as u64;
        let offset = self
            .stream_offset
            .checked_add(HEADER_SIZE as u64)
            .and_then(|value| value.checked_add(sequence.checked_mul(stride)?))
            .ok_or_else(|| anyhow!("RGX keyed payload frame offset overflow"))?;
        self.file.seek(SeekFrom::Start(offset))?;
        let mut frame = [0u8; FRAME_HEADER_SIZE];
        self.file.read_exact(&mut frame)?;
        let (actual, plaintext_len, ciphertext_len, _) =
            decode_frame_header(&frame, &self.header)?;
        if actual != sequence {
            bail!("RGX keyed payload frame sequence mismatch");
        }
        let mut ciphertext = vec![0u8; ciphertext_len as usize];
        self.file.read_exact(&mut ciphertext)?;
        let nonce = frame_nonce(&self.header.nonce_prefix, sequence);
        let aad = frame_aad(&self.header_bytes, &frame);
        let plaintext = self
            .cipher
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| anyhow!("RGX keyed payload authentication failed"))?;
        if plaintext.len() != plaintext_len as usize {
            bail!("RGX keyed payload frame length verification failed");
        }
        self.cached_plaintext = plaintext;
        self.cached_sequence = Some(sequence);
        Ok(())
    }
}

impl Read for KeyedReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() || self.position >= self.plaintext_len {
            return Ok(0);
        }
        let mut written = 0usize;
        while written < output.len() && self.position < self.plaintext_len {
            let sequence = self.position / self.header.frame_size as u64;
            let within = (self.position % self.header.frame_size as u64) as usize;
            self.load_frame(sequence).map_err(to_io_error)?;
            let available = self.cached_plaintext.len().saturating_sub(within);
            if available == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "invalid RGX keyed payload frame length",
                ));
            }
            let take = available.min(output.len() - written);
            output[written..written + take]
                .copy_from_slice(&self.cached_plaintext[within..within + take]);
            written += take;
            self.position += take as u64;
        }
        Ok(written)
    }
}

impl Seek for KeyedReader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let next = match from {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::Current(value) => i128::from(self.position) + i128::from(value),
            SeekFrom::End(value) => i128::from(self.plaintext_len) + i128::from(value),
        };
        if next < 0 || next > i128::from(self.plaintext_len) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid RGX keyed payload seek",
            ));
        }
        self.position = next as u64;
        Ok(self.position)
    }
}

fn encode_header(header: &StreamHeader) -> [u8; HEADER_SIZE] {
    let mut bytes = [0u8; HEADER_SIZE];
    bytes[0..4].copy_from_slice(&KEYED_MAGIC);
    bytes[4..6].copy_from_slice(&VERSION.to_le_bytes());
    bytes[8..12].copy_from_slice(&header.frame_size.to_le_bytes());
    bytes[12..28].copy_from_slice(&header.nonce_prefix);
    bytes[28..60].copy_from_slice(&header.envelope_hash);
    bytes
}

fn decode_header(bytes: &[u8; HEADER_SIZE]) -> Result<StreamHeader> {
    if bytes[0..4] != KEYED_MAGIC {
        bail!("not an RGX keyed payload stream");
    }
    let version = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
    if version != VERSION {
        bail!("unsupported RGX keyed payload version {version}");
    }
    if bytes[6..8] != [0u8; 2] || bytes[60..64] != [0u8; 4] {
        bail!("unsupported RGX keyed payload header flags");
    }
    let frame_size = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if !(64 * 1024..=4 * 1024 * 1024).contains(&frame_size) {
        bail!("RGX keyed payload frame size is outside the accepted range");
    }
    let mut nonce_prefix = [0u8; NONCE_PREFIX_SIZE];
    nonce_prefix.copy_from_slice(&bytes[12..28]);
    let mut envelope_hash = [0u8; 32];
    envelope_hash.copy_from_slice(&bytes[28..60]);
    Ok(StreamHeader {
        frame_size,
        nonce_prefix,
        envelope_hash,
    })
}

fn encode_frame_header(
    sequence: u64,
    plaintext_len: u32,
    ciphertext_len: u32,
    last: bool,
) -> [u8; FRAME_HEADER_SIZE] {
    let mut bytes = [0u8; FRAME_HEADER_SIZE];
    bytes[0..4].copy_from_slice(&FRAME_MAGIC);
    bytes[4] = u8::from(last);
    bytes[8..16].copy_from_slice(&sequence.to_le_bytes());
    bytes[16..20].copy_from_slice(&plaintext_len.to_le_bytes());
    bytes[20..24].copy_from_slice(&ciphertext_len.to_le_bytes());
    bytes
}

fn decode_frame_header(
    bytes: &[u8; FRAME_HEADER_SIZE],
    header: &StreamHeader,
) -> Result<(u64, u32, u32, bool)> {
    if bytes[0..4] != FRAME_MAGIC {
        bail!("invalid RGX keyed payload frame marker");
    }
    if bytes[4] > 1 || bytes[5..8] != [0u8; 3] {
        bail!("invalid RGX keyed payload frame flags");
    }
    let last = bytes[4] == 1;
    let sequence = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    let plaintext_len = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    let ciphertext_len = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
    if plaintext_len > header.frame_size {
        bail!("RGX keyed payload frame declares too much plaintext");
    }
    if !last && plaintext_len != header.frame_size {
        bail!("non-final RGX keyed payload frame is not full sized");
    }
    if ciphertext_len != plaintext_len + TAG_SIZE as u32 {
        bail!("RGX keyed payload ciphertext length is invalid");
    }
    Ok((sequence, plaintext_len, ciphertext_len, last))
}

fn frame_nonce(prefix: &[u8; NONCE_PREFIX_SIZE], sequence: u64) -> [u8; 24] {
    let mut nonce = [0u8; 24];
    nonce[..NONCE_PREFIX_SIZE].copy_from_slice(prefix);
    nonce[NONCE_PREFIX_SIZE..].copy_from_slice(&sequence.to_le_bytes());
    nonce
}

fn frame_aad(header: &[u8; HEADER_SIZE], frame_header: &[u8; FRAME_HEADER_SIZE]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(HEADER_SIZE + FRAME_HEADER_SIZE);
    aad.extend_from_slice(header);
    aad.extend_from_slice(frame_header);
    aad
}

fn to_io_error(error: anyhow::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}
