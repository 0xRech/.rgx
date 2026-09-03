# .rgx Roadmap

## v0.1 — Format foundation

- Custom RGX container
- Zstandard compression
- Store fallback for incompressible files
- BLAKE3 per-file integrity
- Pack / extract / list / info / verify CLI
- Extraction path-safety checks
- Roundtrip tests and CI

## v0.2 — Private archives

- Password-protected archive profile
- Argon2id password-based key derivation
- XChaCha20-Poly1305 authenticated encryption
- Encrypted manifest / file names / directory structure
- Explicit cryptographic version and parameters in the format
- Security test vectors

No custom cryptographic primitive will be introduced.

## v0.3 — Smarter compression

- Content-defined chunking
- Chunk-level BLAKE3 identifiers
- Deduplication inside an archive
- Adaptive compression decisions
- Benchmark command against ZIP and other local tools where available

Deduplication and encrypted archives must be designed together to avoid leaking useful content fingerprints.

## v0.4 — Archive index and selective access

- Footer index
- Fast file lookup
- Selective extraction without scanning every payload
- Streaming reads

## v0.5 — Snapshots

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
