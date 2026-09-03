# .rgx Roadmap

## v0.1 — Format foundation ✅

- Custom RGX container
- Zstandard compression
- Store fallback for incompressible files
- BLAKE3 per-file integrity
- Pack / extract / list / info / verify CLI
- Extraction path-safety checks
- Roundtrip tests and CI

## v0.2 — Streaming, chunking and deduplication ✅

- Streaming file reads instead of loading complete files into memory
- Content-defined chunking
- Rolling-hash chunk boundaries
- Archive-wide BLAKE3 chunk identifiers
- Chunk-level deduplication
- Per-chunk and per-file integrity verification
- Deduplication statistics in `rgx info`
- Protection against writing an archive into its own source tree
- Corruption and deduplication tests

## v0.3 — Private archives

- Password-protected archive profile
- Argon2id password-based key derivation
- XChaCha20-Poly1305 authenticated encryption
- Encrypted manifest / file names / directory structure
- Explicit cryptographic version and parameters in the format
- Security test vectors
- Design deduplication and encryption together to avoid leaking useful content fingerprints

No custom cryptographic primitive will be introduced.

## v0.4 — Smarter compression and selective access

- Adaptive codec selection
- Footer index
- Fast file lookup
- Selective extraction without scanning all metadata
- Streaming reads from individual archived files
- Benchmark command against ZIP and other local tools where available

## v0.5 — Snapshots and incremental archives

- Incremental updates
- Snapshot history
- Delta-aware storage
- Archive diff

## Later

- Recovery/parity blocks
- Public-key recipient encryption
- Signatures
- Mountable archives
- GUI
- Windows/macOS/Linux release packages
- Long-term stable 1.0 specification
