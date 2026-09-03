# Security Policy

## Current status

RGX v0.2 is experimental software and **does not provide confidentiality**. It includes BLAKE3 integrity checks at both chunk and reconstructed-file level to detect accidental corruption, but those hashes are not an authentication mechanism against an attacker who can rewrite the archive.

RGX v0.2 also performs archive-wide deduplication using plaintext BLAKE3 chunk identifiers. This is useful for compact storage, but those identifiers can reveal equality relationships between chunks. Therefore the current format must not be described as privacy-preserving or confidential.

Do not store confidential information in RGX v0.2 unless the `.rgx` file is protected by a separate, established encryption layer.

## Current defensive measures

The reference reader and extractor are designed to reject malformed or dangerous archive structures, including:

- absolute and parent-traversal paths
- duplicate or ambiguous archive paths
- file paths reused as parent directories
- unsupported or oversized chunk declarations
- unknown or forward chunk references
- duplicate and unreferenced chunk records
- inconsistent footer statistics
- trailing data after the footer
- extraction into an already existing destination

The writer also refuses to create the output archive inside the directory tree being packed.

## Planned cryptography

The encrypted RGX profile is planned around established, reviewed primitives such as Argon2id for password-based key derivation and XChaCha20-Poly1305 for authenticated encryption. RGX will not invent a proprietary cipher.

The encrypted format must explicitly address the interaction between deduplication and confidentiality before deduplication is enabled for private archives. Cryptographic parameters, format versioning, and test vectors will be defined before RGX is described as suitable for confidential data.

## Reporting vulnerabilities

Please avoid publishing exploit details in a public issue before maintainers have had a reasonable opportunity to review the report. For now, use GitHub's private vulnerability reporting feature if it is enabled for this repository.
