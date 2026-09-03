# .rgx

**.rgx** is an experimental archive format and reference implementation from the Rech Group ecosystem, focused on three goals: **compact storage, privacy-ready design, and resilient verification**.

> **Status: v0.1 / experimental.** The current version is a format and CLI foundation. It is **not yet encrypted** and must not be presented as a secure container for confidential data. Encryption is intentionally scheduled after the core format has proven stable.

## What exists in v0.1

- A real custom `.rgx` binary container — not a renamed ZIP file.
- `rgx pack` and `rgx extract` for lossless archive roundtrips.
- Zstandard compression with automatic store fallback when compression would make a file larger.
- BLAKE3 hashes for per-file integrity verification.
- `rgx list`, `rgx info`, and `rgx verify`.
- Path traversal defenses during extraction.
- Format versioning and reserved flags for future capabilities.
- Automated format, lint, and test checks in GitHub Actions.

## CLI

```bash
rgx pack ./project project.rgx
rgx pack ./project project.rgx --level 12
rgx list project.rgx
rgx info project.rgx
rgx verify project.rgx
rgx extract project.rgx ./restore
```

## Design principles

### Compact
RGX v0.1 uses Zstandard and stores a file uncompressed when compression would be larger. Future versions will add content-defined chunking, deduplication, adaptive codec selection, and delta storage.

### Private
Privacy is a format requirement, but v0.1 does not claim confidentiality. The planned encrypted profile will protect file data, names, directory structure, and metadata using established cryptographic primitives rather than custom cryptography.

### Resilient
Each file carries a BLAKE3 digest. `rgx verify` decompresses and checks every file, allowing corruption to be detected before extraction. Recovery/parity data is planned for a later format revision.

## Repository layout

```text
src/
  archive.rs       packing, extraction and verification
  format.rs        binary format primitives
  lib.rs           library entry point
  main.rs          rgx CLI

docs/
  FORMAT.md        current binary format specification
  ROADMAP.md       staged development plan

tests/
  roundtrip.rs     lossless archive tests
```

## Current limitations

- No encryption yet.
- No symbolic-link support.
- Paths must be valid UTF-8.
- File permissions and timestamps are not yet preserved.
- No deduplication, snapshots, recovery blocks, or random-access index yet.
- The format may change before a stable 1.0 specification.

## Security
Do not use v0.1 as a replacement for an encrypted archive format. See [SECURITY.md](SECURITY.md) for the current security model and reporting guidance.

## Format
The current format is documented in [docs/FORMAT.md](docs/FORMAT.md).

## License
MIT. See [LICENSE](LICENSE).
