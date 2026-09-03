# Security Policy

## Current status

RGX v0.1 is experimental software and **does not provide confidentiality**. It includes BLAKE3 integrity checks to detect accidental corruption, but those hashes are not an authentication mechanism against an attacker who can modify the archive.

Do not store confidential information in RGX v0.1 unless the `.rgx` file is protected by a separate, established encryption layer.

## Planned cryptography

The encrypted RGX profile is planned around established, reviewed primitives such as Argon2id for password-based key derivation and XChaCha20-Poly1305 for authenticated encryption. RGX will not invent a proprietary cipher.

The encrypted format will receive explicit versioning and test vectors before it is described as suitable for confidential data.

## Reporting vulnerabilities

Please avoid publishing exploit details in a public issue before maintainers have had a reasonable opportunity to review the report. For now, use GitHub's private vulnerability reporting feature if it is enabled for this repository.
