<p align="center">
  <img src="https://github.com/user-attachments/assets/e04d753b-8016-4db4-8cf0-32f2b79c77ec" alt=".RGX — Rech Group Archive" width="760" />
</p>

# .rgx

**.rgx** is an experimental archive format and reference implementation from the Rech Group ecosystem, focused on three goals: **compact storage, private archives, and resilient verification**.

> **Status: v0.4 / experimental.** RGX now supports password-protected private archives using established cryptographic primitives. The implementation has automated roundtrip, wrong-password, tamper-detection, plaintext-leak, formatting, lint, and test coverage, but it has **not undergone an independent security audit**.

## What exists in v0.4

- A real custom `.rgx` binary container — not a renamed ZIP file.
- Streaming source-file reads.
- Content-defined chunking with a rolling hash.
- Archive-wide BLAKE3 chunk identifiers and deduplication.
- Zstandard compression with automatic store fallback for incompressible chunks.
- Per-chunk and per-file BLAKE3 integrity verification.
- **Private Mode** using Argon2id + XChaCha20-Poly1305 authenticated encryption.\n- Private packing streams directly into authenticated frames; private reads use seekable, random-access frame decryption without a plaintext temporary archive.\n- Selective extraction with `rgx extract ARCHIVE OUTPUT --path ARCHIVE_PATH`.\n- Fast catalog operations with `rgx find` and verified file streaming with `rgx cat`.
- Private archives encrypt the complete inner RGX container, including file names, paths, directory structure, chunk identifiers, deduplication metadata, and file data.
- Passwords are prompted without echo; automation can use an environment variable instead of putting a password on the command line.
- `rgx pack`, `rgx extract`, `rgx list`, `rgx info`, and `rgx verify` automatically understand plain and private RGX archives.
- `rgx benchmark` compares RGX against a built-in ZIP/Deflate baseline and automatically includes 7-Zip when a `7z`, `7zz`, or `7za` executable is available.
- Path-traversal defenses, duplicate-path checks, corruption detection, and refusal to overwrite an existing extraction target.

## CLI

Create a normal RGX archive:

```bash
rgx pack ./project project.rgx
rgx pack ./project project.rgx --level 12
```

Create a private RGX archive:

```bash
rgx pack ./project project.rgx --private
```

RGX prompts for the password twice when creating a private archive. To use a secret supplied by automation without exposing it in the process command line:

```bash
RGX_PASSWORD='your passphrase' rgx pack ./project project.rgx --private --password-env RGX_PASSWORD
```

The normal commands automatically detect whether an archive is private:

```bash
rgx list project.rgx
rgx info project.rgx
rgx verify project.rgx
rgx extract project.rgx ./restore
```

For a private archive they prompt for the password unless `--password-env NAME` is supplied.

## Benchmark

Run a local comparison on your own files:

```bash
rgx benchmark ./project
```

The benchmark always measures **RGX** and a built-in **ZIP/Deflate** implementation. If 7-Zip is installed and available as `7z`, `7zz`, or `7za`, RGX also measures a normal LZMA2 (`-mx=5`) archive automatically.

Add Private Mode to the same run:

```bash
rgx benchmark ./project --private
```

The Private Mode benchmark uses a temporary internal benchmark password because all benchmark archives are created in a temporary workspace and deleted after the run. No user password is required or stored.

Example output shape:

```text
RGX Benchmark
Input: 8.42 GiB / 18492 files

Method                       Size       Pack    Extract   Pack MiB/s   Extr MiB/s
--------------------------------------------------------------------------------------
RGX                       5.34 GiB     51.80s     21.60s        166.4        399.3
RGX Private               5.35 GiB     58.40s     27.10s        147.6        318.2
ZIP (Deflate)             6.81 GiB     42.10s     18.40s        204.8        468.5
7-Zip (LZMA2 normal)      5.92 GiB    128.70s     31.20s         67.0        276.3
```

Those numbers are illustrative only. `rgx benchmark` reports wall-clock measurements from the machine and storage device on which it is run, so results should be compared on the same hardware and data set.

## Design principles

### Compact

RGX splits files into content-defined chunks. Identical chunks are stored only once across the archive, while each unique chunk is independently tested with Zstandard. If compression would make a chunk larger, RGX stores it unchanged.

This is especially useful for backups, copied files, related project trees, and files that contain large regions of shared content.

### Private

Private Mode first builds the normal deduplicated RGX representation and then authenticates and encrypts that entire representation. As a result, the outer `.rgx` file does not expose plaintext file names, directory names, BLAKE3 chunk identifiers, or file contents.

The current cryptographic profile uses:

```text
KDF:               Argon2id
Memory:            64 MiB
Iterations:        3
Parallelism:       1
AEAD:              XChaCha20-Poly1305
Encrypted frames:  1 MiB
```

Every encrypted frame uses a unique nonce and authenticates both its frame metadata and the private-envelope header. Reordered, modified, truncated, appended, or incorrectly decrypted data is rejected.

### Resilient

Inside the encrypted or plain container, every unique chunk has a BLAKE3 digest and every file has an independent BLAKE3 digest over its reconstructed contents. `rgx verify` validates private-envelope authentication as well as the inner RGX structure and hashes.

## Content-defined chunking

The reference writer currently uses:

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
  benchmark.rs     local RGX / ZIP / optional 7-Zip benchmark engine
  chunker.rs       content-defined chunk boundary logic
  format.rs        RGX v0.2 inner binary format primitives
  private.rs       v0.3 Argon2id + XChaCha20-Poly1305 private envelope
  lib.rs           library entry point
  main.rs          rgx CLI

docs/
  FORMAT.md        binary format and private-envelope specification
  ROADMAP.md       staged development plan

tests/
  benchmark.rs     built-in benchmark coverage
  roundtrip.rs     roundtrip, deduplication and corruption tests
  private.rs       private-mode, wrong-password, tamper and leak tests
```

## Current limitations

- RGX is pre-1.0 and the format may still change.
- Private Mode has not been independently audited.
- **v0.3 currently materializes the decrypted inner RGX container inside a temporary working directory during private operations.** The next hardening stage will replace this with seekable encrypted I/O so no complete plaintext container needs to exist on disk.
- The outer encrypted envelope still reveals approximate total archive size and its public cryptographic parameters.
- No symbolic-link support.
- Paths must be valid UTF-8.
- File permissions and timestamps are not yet preserved.
- No snapshots, recovery blocks, random-access footer index, mount support, or recipient public-key encryption yet.
- The v0.2 inner container is not backwards-compatible with experimental v0.1 archives.

## Security

Private Mode is designed around established primitives rather than proprietary cryptography, but RGX v0.4 is still experimental software. Read [SECURITY.md](SECURITY.md) before using it for sensitive information.

## Format

The v0.2 inner format and v0.3 private envelope are documented in [docs/FORMAT.md](docs/FORMAT.md).

## License

MIT. See [LICENSE](LICENSE).
