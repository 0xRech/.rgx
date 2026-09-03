<p align="center">
  <img src="https://github.com/user-attachments/assets/e04d753b-8016-4db4-8cf0-32f2b79c77ec" alt=".RGX — Rech Group Archive" width="760" />
</p>

# .rgx

**.rgx** is an experimental archive format and reference implementation from the Rech Group ecosystem, focused on three goals: **compact storage, privacy-ready design, and resilient verification**.

> **Status: v0.2 / experimental.** RGX now has a streaming, content-defined chunking and deduplication foundation. It is **not yet encrypted** and must not yet be used as a confidentiality layer for sensitive data.

## What exists in v0.2

- A real custom `.rgx` binary container — not a renamed ZIP file.
- Streaming file reads: large files are no longer loaded completely into memory.
- Content-defined chunking with a rolling hash.
- Archive-wide BLAKE3 chunk identifiers and deduplication.
- Zstandard compression with automatic store fallback for incompressible chunks.
- Per-chunk and per-file BLAKE3 integrity verification.
- `rgx pack`, `rgx extract`, `rgx list`, `rgx info`, and `rgx verify`.
- Extraction path traversal defenses and refusal to overwrite an existing extraction target.
- Protection against creating the output archive inside the directory being packed.
- Versioned format specification and automated Rust format/lint/test checks.

## CLI

```bash
rgx pack ./project project.rgx
rgx pack ./project project.rgx --level 12
rgx list project.rgx
rgx info project.rgx
rgx verify project.rgx
rgx extract project.rgx ./restore
```

`rgx info` reports both physical chunk storage and logical bytes saved through deduplication.

## Design principles

### Compact
RGX v0.2 splits files into content-defined chunks. Identical chunks are stored only once across the archive, while each unique chunk is compressed independently with Zstandard. If compression would make a chunk larger, RGX stores it unchanged.

This is especially useful for backups, copied files, related project trees, and files that contain large regions of shared content.

### Privacy-ready
Privacy is a format requirement, but v0.2 does not claim confidentiality. The planned encrypted profile will protect file data, names, directory structure, and metadata using established cryptographic primitives rather than custom cryptography.

Deduplication and encryption will be designed together so the encrypted profile does not accidentally expose useful content fingerprints.

### Resilient
Every unique chunk has a BLAKE3 digest and every file has an independent BLAKE3 digest over its reconstructed contents. `rgx verify` validates the chunk data and the final file contents before an archive is trusted.

## Content-defined chunking

The v0.2 reference writer currently uses:

```text
rolling window:   64 bytes
minimum chunk:    64 KiB
target chunk:    256 KiB
maximum chunk:      1 MiB
```

Unlike fixed-size splitting, content-defined boundaries can re-synchronize after bytes are inserted or removed. That allows unchanged regions of related files to resolve to the same chunks and be deduplicated.

## Repository layout

```text
src/
  archive.rs       packing, extraction, deduplication and verification
  chunker.rs       content-defined chunk boundary logic
  format.rs        RGX binary format primitives
  lib.rs           library entry point
  main.rs          rgx CLI

docs/
  FORMAT.md        current binary format specification
  ROADMAP.md       staged development plan

tests/
  roundtrip.rs     roundtrip, deduplication and corruption tests
```

## Current limitations

- No encryption yet.
- No symbolic-link support.
- Paths must be valid UTF-8.
- File permissions and timestamps are not yet preserved.
- No snapshots, recovery blocks, random-access footer index, or mount support yet.
- v0.2 is a draft format and is not backwards-compatible with the experimental v0.1 container.
- The format may change before a stable 1.0 specification.

## Security

Do not use v0.2 as a replacement for an encrypted archive format. See [SECURITY.md](SECURITY.md) for the current security model and reporting guidance.

## Format

The current format is documented in [docs/FORMAT.md](docs/FORMAT.md).

## License

MIT. See [LICENSE](LICENSE).
