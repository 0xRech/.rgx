# Security Policy

## Current status

RGX v0.3 is experimental software. It now provides an **experimental password-protected private mode** based on established cryptographic primitives, but it has not undergone an independent security audit and should not yet be treated as a mature replacement for long-established encrypted archive tools in high-risk environments.

Plain RGX archives remain non-confidential. Their BLAKE3 chunk identifiers, paths, structure, and file metadata are visible by design.

## Private Mode cryptographic profile

A private RGX archive encrypts the complete inner RGX container. This includes:

- file contents
- file names and paths
- directory structure
- BLAKE3 chunk identifiers
- per-file hashes
- chunk references and deduplication relationships
- inner footer statistics

The current profile uses:

- **Argon2id** password-based key derivation
- 64 MiB Argon2 memory cost
- 3 Argon2 iterations
- parallelism 1
- a random 16-byte salt per archive
- **XChaCha20-Poly1305** authenticated encryption
- a random 16-byte nonce prefix per archive plus a unique 64-bit frame sequence
- 1 MiB encrypted frames

The exact private-envelope header and authenticated-data construction are documented in `docs/FORMAT.md`.

RGX does not implement a proprietary cipher.

## Authentication and corruption handling

Each private frame authenticates the complete private-envelope header and its own frame header as associated data. This binds the cryptographic parameters, sequence number, plaintext length, ciphertext length, and final-frame marker to the ciphertext.

The reader rejects:

- incorrect passwords
- modified ciphertext
- modified authenticated frame metadata
- frame reordering
- missing or truncated frames
- trailing data after the authenticated final frame
- unsupported or unreasonable Argon2 parameters

After decryption, the normal RGX parser additionally validates BLAKE3 chunk hashes, reconstructed-file hashes, paths, deduplication references, and footer statistics.

## Metadata exposure

Private Mode hides the inner RGX structure, but the outer envelope necessarily exposes some information required before password verification:

- that the file is a private RGX archive
- the private-envelope format version
- Argon2 parameters
- encrypted frame size
- salt and nonce prefix
- approximate total encrypted archive size and therefore an approximate frame count

The salt and nonce prefix are random public values and are not secrets.

## Important v0.3 limitation: temporary plaintext container

The v0.3 reference implementation currently creates the deduplicated inner RGX container inside a temporary working directory before encryption, and reconstructs that inner container in a temporary working directory during private reads/extraction.

The temporary directory is created using the platform temporary-file facilities and is removed when the operation completes, but **RGX does not claim secure deletion of those temporary plaintext bytes**. On SSDs, copy-on-write filesystems, snapshots, swap, backups, forensic storage, or a compromised host, deleted temporary data may remain recoverable.

Therefore v0.3 Private Mode primarily protects the final `.rgx` archive at rest and in transit. Users with a threat model that forbids temporary plaintext storage should wait for the planned seekable encrypted-I/O implementation.

## Current defensive measures

The reference reader and extractor also reject malformed or dangerous inner archive structures, including:

- absolute and parent-traversal paths
- duplicate or ambiguous archive paths
- file paths reused as parent directories
- unsupported or oversized chunk declarations
- unknown or forward chunk references
- duplicate and unreferenced chunk records
- inconsistent footer statistics
- trailing data after the inner footer
- extraction into an already existing destination

The writer refuses to create the final archive inside the directory tree being packed.

## Password handling

The CLI prompts for passwords without echoing them. Passwords are not accepted as literal command-line values. Automated use may supply the password through an explicitly named environment variable with `--password-env NAME`.

Environment variables may still be observable to privileged local processes or captured by CI configuration. Treat them as secrets and use the secret-management facilities of the execution environment.

## Reporting vulnerabilities

Please avoid publishing exploit details in a public issue before maintainers have had a reasonable opportunity to review the report. Use GitHub's private vulnerability reporting feature if it is enabled for this repository.
