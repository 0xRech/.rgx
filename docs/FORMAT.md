# RGX Format Specification — Draft 0.1

This document defines the binary layout emitted by the v0.1 reference implementation. All multi-byte integers are **little-endian**. Paths are UTF-8 and use `/` separators.

The specification is intentionally versioned from the first byte so future versions can evolve without silently misreading older archives.

## Archive header

| Offset | Size | Field | Value / meaning |
| --- | ---: | --- | --- |
| 0 | 4 | Magic | `52 47 58 00` (`RGX\\0`) |
| 4 | 2 | Major version | `0` |
| 6 | 2 | Minor version | `1` |
| 8 | 4 | Flags | `0` in v0.1 |
| 12 | 4 | Reserved | `0` |

The header is followed by zero or more entries.

## Entry

Each entry starts with `ENTR` and contains a fixed header, a UTF-8 path and, for files, the payload.

| Field | Size | Meaning |
| --- | ---: | --- |
| Entry magic | 4 | ASCII `ENTR` |
| Kind | 1 | `0` directory, `1` file |
| Compression | 1 | `0` store, `1` Zstandard |
| Reserved | 2 | `0` |
| Path length | 4 | UTF-8 path length in bytes |
| Original size | 8 | Uncompressed file size |
| Payload size | 8 | Stored payload size |
| BLAKE3 | 32 | Digest of the uncompressed file; zeroes for directories |
| Path | variable | UTF-8 relative path |
| Payload | variable | File data according to compression field |

Directory entries have original size and payload size set to zero and carry no payload.

## Footer

| Field | Size | Meaning |
| --- | ---: | --- |
| Footer magic | 4 | ASCII `RGXF` |
| Entry count | 8 | Number of entries preceding the footer |

A missing footer is considered corruption/truncation.

## Path safety requirements

Readers MUST reject absolute paths and `..` components. The reference implementation also rejects backslashes to keep archived paths platform-neutral and avoid Windows path ambiguity.

## Compression behavior

The reference writer compresses regular files with Zstandard. If the compressed representation is not smaller than the source data, the writer stores the original bytes with compression method `0`.

This rule is a property of the reference writer, not a requirement for future compatible writers.

## Integrity

The 32-byte BLAKE3 digest is calculated over the **uncompressed** file contents. A conforming verifier should decompress the payload, confirm the exact original size, then compare the digest.

BLAKE3 provides integrity detection here; it does **not** authenticate the archive against a malicious party. Authenticated encryption/signatures are planned for later versions.

## Compatibility policy

Before RGX 1.0 this format is a draft and may change. Readers must reject unsupported newer versions instead of guessing.
