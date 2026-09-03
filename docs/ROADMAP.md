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
- Complete encryption of the inner RGX container, including file names, paths, directory structure, hashes, chunk identifiers and deduplication metadata
- Explicit private-envelope format version and KDF/AEAD parameters
- Unique frame nonces and authenticated frame metadata
- Password prompting without command-line password values
- Optional environment-variable secret input for automation
- Wrong-password, tamper-detection, plaintext-leak and private roundtrip tests
- Built-in `rgx benchmark` comparison against ZIP/Deflate
- Optional automatic comparison against an installed 7-Zip executable
- Optional Private Mode timing in the same benchmark run
- Archive size, wall-clock time, throughput and RGX deduplication statistics

No custom cryptographic primitive is introduced.

## v0.4 — Private-mode hardening and selective access

- Replace temporary plaintext inner-container files with seekable encrypted I/O
- Stream private packing directly into the encrypted envelope
- Random-access decryption of authenticated frames
- Footer index
- Fast file lookup
- Selective extraction without reconstructing the complete archive
- Streaming reads from individual archived files
- Additional cryptographic test vectors and fuzzing

## v0.5 — Smarter compression

- Adaptive codec selection
- Workload-aware compression profiles
- Reproducible benchmark corpus definitions
- Compression/deduplication telemetry for longitudinal benchmark reports

## v0.6 — Snapshots and incremental archives

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
