# RGX Format Specification — Draft 0.2

This document defines the binary layout emitted by the RGX v0.2 reference implementation. All multi-byte integers are **little-endian**. Paths are UTF-8 and use `/` separators.

RGX is still pre-1.0. Draft revisions may be intentionally incompatible while the container design is being stabilized.

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

The v0.2 reference writer only emits non-empty chunks up to 1 MiB. A stored chunk has `payload size == original size`. A Zstandard chunk is emitted only when it is smaller than the original chunk.

A BLAKE3 chunk identifier may appear as a `CHNK` record only once in an archive.

## File record

A file is represented by metadata plus an ordered list of BLAKE3 chunk identifiers.

| Field | Size | Meaning |
| --- | ---: | --- |
| Magic | 4 | ASCII `FILE` |
| Path length | 4 | UTF-8 path length in bytes |
| Chunk count | 4 | Number of 32-byte chunk references |
| Original size | 8 | Reconstructed file size |
| BLAKE3 | 32 | Digest of the full reconstructed file |
| Path | variable | UTF-8 relative path |
| Chunk references | `32 × count` | Ordered BLAKE3 chunk identifiers |

The v0.2 reference reader requires each file reference to point to a chunk already defined earlier in the archive. Empty files contain zero chunk references and use the BLAKE3 digest of an empty byte string.

## Directory record

| Field | Size | Meaning |
| --- | ---: | --- |
| Magic | 4 | ASCII `DIRE` |
| Path length | 4 | UTF-8 path length in bytes |
| Reserved | 4 | `0` |
| Path | variable | UTF-8 relative path |

Directory records carry no payload.

## Footer

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

RGX v0.2 uses the BLAKE3 digest of an uncompressed chunk as its archive-local identity.

When a chunk digest has already been stored, later files write only the 32-byte chunk reference instead of writing the same chunk payload again. The reader reconstructs files by resolving those references in order.

The current unencrypted format therefore exposes chunk fingerprints. This is acceptable for the experimental non-confidential profile, but the future encrypted profile must address the information leakage implications of deduplication.

## Reference chunking behavior

Chunk boundaries are a writer implementation detail rather than a container-format requirement. The v0.2 reference writer uses content-defined chunking with:

- 64-byte rolling window
- 64 KiB minimum chunk size
- 256 KiB target boundary probability
- 1 MiB maximum chunk size

A boundary is accepted after the minimum size when the rolling state matches the target mask, or unconditionally at the maximum size.

Because boundaries are content-defined rather than fixed offsets, a file can often re-synchronize after inserted or removed bytes and reuse later chunks from related files.

## Compression behavior

Each unique chunk is independently tested with Zstandard. If the compressed representation is smaller, compression method `1` is used. Otherwise the original chunk is stored with compression method `0`.

## Integrity

Two levels of BLAKE3 verification are present:

1. Each unique chunk is hashed over its uncompressed bytes.
2. Each file is independently hashed over the fully reconstructed file contents.

This detects accidental corruption and malformed reconstruction. These hashes do **not** authenticate an archive against an attacker capable of rewriting both data and hashes.

## Path safety requirements

Readers MUST reject absolute paths, empty components, `.` components, `..` components, and ambiguous platform path syntax. The reference reader rejects backslashes in stored paths and refuses duplicate paths or a file path that is also used as the parent of another entry.

The reference extractor requires a new output directory and does not overwrite an existing extraction target.

## Compatibility policy

RGX v0.2 is a draft and the reference reader currently expects exactly format version `0.2`. Experimental v0.1 archives are not guaranteed to open in v0.2.

Before RGX 1.0, format revisions may be breaking. After a stable 1.0 specification, compatibility policy will be tightened.
