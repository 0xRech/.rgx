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

- Streaming source-file reads instead of loading complete files into memory
- Content-defined chunking
- Rolling-hash chunk boundaries
- Archive-wide BLAKE3 chunk identifiers
- Chunk-level deduplication
- Per-chunk and per-file integrity verification
- Deduplication statistics in `rgx info`
- Protection against writing an archive into its own source tree
- Corruption and deduplication tests

## v0.3 — Private archives and benchmark tooling ✅

- Password-protected `--private` archive profile
- Argon2id password-based key derivation
- XChaCha20-Poly1305 authenticated encryption
- Complete encryption of inner metadata and contents
- Explicit private-envelope version and KDF/AEAD parameters
- Password prompting and environment-variable input for automation
- Wrong-password, tamper-detection, plaintext-leak and private roundtrip tests
- Built-in benchmarking against ZIP/Deflate and optional 7-Zip

## v0.4 — Private-mode hardening and selective access ✅

- Seekable encrypted I/O without a complete plaintext temporary archive
- Direct streaming into authenticated encrypted frames
- Random-access decryption of authenticated frames
- Selective extraction
- `find`, `cat`, `list`, `info`, and `verify` through encrypted streams
- Cross-platform CI, dependency audits and parser fuzzing
- Relative-output path regression coverage

Persisted footer indexing remains future work.

## v0.5 — Recipient identities and authenticity 🚧

`v0.5.0-alpha1` is currently implemented on the `test` branch.

- X25519 RGX recipient identities ✅
- Random 256-bit per-archive content key ✅
- Multiple recipient key slots ✅
- Native `RGXR` recipient envelope v2 ✅
- Native seekable `RGXK` XChaCha20-Poly1305 payload stream ✅
- Automatic lookup of matching local RGX identities ✅
- Optional Argon2id password fallback ✅
- Optional Argon2id + XChaCha20-Poly1305 protection for private identity files ✅
- v2 public key files containing X25519 and Ed25519 public material ✅
- Detached Ed25519 archive signatures ✅
- Recipient-envelope fuzz target ✅
- End-to-end CLI tests for recipient unlock, protected keys and signatures ✅
- Independent cryptographic review ⏳
- Windows-specific hardened private-key ACL handling ⏳
- OS keychain / TPM / hardware-token integration ⏳

## v0.6 — Smarter compression

- Adaptive codec selection
- Workload-aware compression profiles
- Reproducible benchmark corpus definitions
- Compression/deduplication telemetry for longitudinal benchmark reports
- Persisted footer/index optimization

## v0.7 — Snapshots and incremental archives

- Incremental updates
- Snapshot history
- Delta-aware storage
- Archive diff

## Later

- Recovery/parity blocks
- Mountable archives
- GUI
- Hardware-backed identities
- Additional platform packaging and integration
- Long-term stable 1.0 specification
