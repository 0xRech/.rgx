# RGX Format Specification — Draft 0.3

RGX v0.3 consists of two layers:

1. the **RGX v0.2 inner archive format**, which provides chunking, compression, deduplication, paths, and integrity metadata;
2. the optional **RGX v0.3 private envelope**, which encrypts and authenticates the complete inner archive.

All multi-byte integers are **little-endian**. RGX is still pre-1.0; draft revisions may intentionally be incompatible while the format is stabilized.

# 1. RGX v0.2 inner archive

## Archive header

| Offset | Size | Field | Value / meaning |
| --- | ---: | --- | --- |
| 0 | 4 | Magic | `52 47 58 00` (`RGX\\0`) |
| 4 | 2 | Major version | `0` |
| 6 | 2 | Minor version | `2` |
| 8 | 4 | Flags | `0` in v0.2 |
| 12 | 4 | Reserved | `0` |

The header is followed by a sequence of chunk, file, and directory records and ends with exactly one footer.

## Chunk record

A unique content chunk is stored using a `CHNK` record.

| Field | Size | Meaning |
| --- | ---: | --- |
| Magic | 4 | ASCII `CHNK` |
| Compression | 1 | `0` store, `1` Zstandard |
| Reserved | 3 | `0` |
| Original size | 8 | Uncompressed chunk size |
| Payload size | 8 | Stored payload size |
| BLAKE3 | 32 | Digest of the uncompressed chunk |
| Payload | variable | Stored or Zstandard-compressed bytes |

The reference writer emits non-empty chunks up to 1 MiB. A stored chunk has `payload size == original size`. A Zstandard chunk is emitted only when it is smaller than the original chunk.

A BLAKE3 chunk identifier may appear as a `CHNK` record only once in an inner archive.

## File record

| Field | Size | Meaning |
| --- | ---: | --- |
| Magic | 4 | ASCII `FILE` |
| Path length | 4 | UTF-8 path length in bytes |
| Chunk count | 4 | Number of 32-byte chunk references |
| Original size | 8 | Reconstructed file size |
| BLAKE3 | 32 | Digest of the full reconstructed file |
| Path | variable | UTF-8 relative path |
| Chunk references | `32 × count` | Ordered BLAKE3 chunk identifiers |

Each chunk reference must resolve to a chunk already defined earlier in the inner archive. Empty files contain zero chunk references and use the BLAKE3 digest of an empty byte string.

## Directory record

| Field | Size | Meaning |
| --- | ---: | --- |
| Magic | 4 | ASCII `DIRE` |
| Path length | 4 | UTF-8 path length in bytes |
| Reserved | 4 | `0` |
| Path | variable | UTF-8 relative path |

## Inner footer

| Field | Size | Meaning |
| --- | ---: | --- |
| Magic | 4 | ASCII `RGXF` |
| Entries | 8 | File + directory entry count |
| Files | 8 | File count |
| Directories | 8 | Directory count |
| Unique chunks | 8 | Number of `CHNK` records |
| Chunk references | 8 | Total chunk references across all files |
| Original bytes | 8 | Sum of logical file sizes |
| Stored payload bytes | 8 | Sum of physical chunk payload sizes |
| Deduplicated bytes | 8 | Logical bytes eliminated by chunk reuse |

Readers verify these footer values against the parsed archive. Missing footers, mismatched statistics, unreferenced chunks, or trailing bytes are treated as corruption.

## Deduplication model

The inner format uses the BLAKE3 digest of an uncompressed chunk as its archive-local identity. If a chunk digest has already been stored, later files reference the existing chunk instead of storing its payload again.

In a plain RGX archive these identifiers are visible. In Private Mode the **entire inner archive**, including all BLAKE3 identifiers and references, is encrypted by the outer envelope.

## Reference chunking behavior

The reference writer uses content-defined chunking with:

- 64-byte rolling window
- 64 KiB minimum chunk size
- 256 KiB target boundary probability
- 1 MiB maximum chunk size

Chunk boundaries are a writer implementation detail rather than a container-format requirement.

## Compression behavior

Each unique chunk is independently tested with Zstandard. If the compressed representation is smaller, compression method `1` is used. Otherwise the original chunk is stored with compression method `0`.

## Inner integrity

Every unique chunk has a BLAKE3 digest of its uncompressed bytes and every file has a BLAKE3 digest of its fully reconstructed contents. These hashes detect corruption and reconstruction errors but do not, by themselves, authenticate a plain archive against a malicious rewriter.

## Path safety

Readers MUST reject absolute paths, empty components, `.` components, `..` components, and ambiguous platform path syntax. The reference reader rejects backslashes, duplicate paths, and file paths reused as parent directories.

The reference extractor requires a new output directory and does not overwrite an existing extraction target.

# 2. RGX v0.3 private envelope

A private RGX file does **not** begin with the normal `RGX\0` inner header. It begins with an `RGXE` envelope and contains authenticated-encryption frames. Decrypting and concatenating the frame plaintext yields one complete RGX v0.2 inner archive.

## Private-envelope header

The private header is exactly **60 bytes**.

| Offset | Size | Field | Value / meaning |
| --- | ---: | --- | --- |
| 0 | 4 | Magic | ASCII `RGXE` |
| 4 | 2 | Major version | `0` |
| 6 | 2 | Minor version | `3` |
| 8 | 1 | KDF | `1` = Argon2id |
| 9 | 1 | AEAD | `1` = XChaCha20-Poly1305 |
| 10 | 2 | Reserved | `0` |
| 12 | 4 | Argon2 memory | KiB |
| 16 | 4 | Argon2 iterations | iteration count |
| 20 | 4 | Argon2 lanes | parallelism |
| 24 | 4 | Frame size | plaintext bytes per normal frame |
| 28 | 16 | Salt | random per archive |
| 44 | 16 | Nonce prefix | random per archive |

The v0.3 reference writer currently emits:

```text
Argon2id memory:   65536 KiB (64 MiB)
Argon2 iterations: 3
Argon2 lanes:      1
Frame size:        1048576 bytes (1 MiB)
```

Readers reject unreasonable KDF and frame-size parameters before allocating the requested resources.

## Key derivation

The user's password is passed to Argon2id using the 16-byte header salt and the parameters stored in the header. The derived output is exactly **32 bytes** and is used as the XChaCha20-Poly1305 key.

The salt is public and is not a secret.

## Encrypted frame

Every frame starts with a fixed **24-byte** frame header followed by authenticated ciphertext.

| Offset | Size | Field | Meaning |
| --- | ---: | --- | --- |
| 0 | 4 | Magic | ASCII `FRAM` |
| 4 | 1 | Final flag | `0` normal, `1` final frame |
| 5 | 3 | Reserved | `0` |
| 8 | 8 | Sequence | monotonically increasing from `0` |
| 16 | 4 | Plaintext length | bytes before encryption |
| 20 | 4 | Ciphertext length | plaintext length + 16-byte Poly1305 tag |
| 24 | variable | Ciphertext | XChaCha20-Poly1305 output |

Every non-final frame must contain exactly the configured frame-size bytes of plaintext. The final frame may be shorter.

## Nonce construction

XChaCha20-Poly1305 requires a 24-byte nonce. RGX constructs it as:

```text
nonce = 16-byte random nonce prefix || 8-byte little-endian frame sequence
```

The sequence number must never repeat within an archive. The nonce prefix is random for each newly created private archive.

## Associated data

For every encrypted frame, the authenticated associated data (AAD) is the exact concatenation:

```text
AAD = 60-byte private-envelope header || 24-byte frame header
```

Therefore the following values are authenticated by XChaCha20-Poly1305 even though they are stored outside the ciphertext:

- envelope version
- KDF and AEAD identifiers
- Argon2 parameters
- salt
- nonce prefix
- frame size
- frame sequence
- final-frame flag
- plaintext and ciphertext lengths

Changing any authenticated value causes decryption to fail.

## Truncation, reordering, and trailing data

Readers require:

- sequences starting at zero and increasing by exactly one;
- exactly one authenticated final frame;
- no bytes after the final frame;
- complete ciphertext for every declared frame.

Reordered, removed, modified, truncated, or appended frame data is rejected.

## Privacy properties

Because the complete RGX v0.2 inner archive is encrypted, an observer without the password does not directly learn:

- file or directory names
- directory structure
- file contents
- inner file sizes and hashes
- BLAKE3 chunk identifiers
- chunk equality relationships
- deduplication statistics

The outer envelope still reveals its public cryptographic parameters and approximate total encrypted size.

## Implementation limitation in v0.3

The v0.3 reference implementation currently materializes the plaintext inner RGX archive in a temporary working directory during private packing and reading. The temporary directory is removed after the operation, but secure deletion is not guaranteed. This is an implementation limitation, not a requirement of the file format.

A future implementation is expected to provide seekable encrypted I/O so the inner container can be consumed without writing the complete plaintext representation to disk.

# Compatibility policy

- Plain RGX data continues to use inner format version `0.2`.
- Private RGX uses envelope version `0.3` around one complete v0.2 inner archive.
- Experimental v0.1 archives are not guaranteed to open.
- Before RGX 1.0, format revisions may be breaking.
