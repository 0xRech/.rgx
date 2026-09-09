# Security Policy

## Current status

RGX `v0.5.0-alpha1` is experimental, unaudited software. It introduces recipient identities, optional protected private-key files, and detached signatures in addition to the existing password Private Mode.

RGX uses established cryptographic primitives, but the complete construction and implementation have not undergone an independent cryptographic or security audit. Do not treat an alpha build as a mature replacement for long-established encrypted archive tools in high-risk environments, and never keep important data only in an experimental RGX archive.

Plain RGX archives are not confidential. Their paths, structure, hashes, chunk identifiers, and file metadata are visible by design.

## Protection modes

RGX currently has two encrypted archive modes.

### Password Private Mode (`RGXE`)

Private Mode encrypts the complete inner RGX container, including file contents, names, paths, directory structure, BLAKE3 identifiers, per-file hashes, chunk references, deduplication relationships, and footer statistics.

Current profile:

- Argon2id password KDF
- 64 MiB memory
- 3 iterations
- parallelism 1
- random 16-byte salt per archive
- XChaCha20-Poly1305 authenticated encryption
- random 16-byte nonce prefix plus a unique 64-bit frame sequence
- 1 MiB authenticated plaintext frames

The writer streams directly into encrypted frames. The reader implements seekable authenticated decryption and does not require a complete plaintext temporary archive.

### Recipient Mode (`RGXR` + `RGXK`)

Recipient Mode generates one random 256-bit archive key and encrypts the inner RGX stream directly with XChaCha20-Poly1305 in the native seekable `RGXK` payload stream.

The archive key is wrapped independently for each X25519 recipient. `RGXR` v2 supports up to 64 recipient slots. An optional password-fallback slot can wrap the same archive key using Argon2id + XChaCha20-Poly1305.

The BLAKE3 digest of the complete recipient envelope is included in the authenticated `RGXK` stream header. Recipient-slot and password-slot metadata are therefore bound to the encrypted payload; envelope modification causes verification to fail.

Recipient slots expose short stable Key-IDs and the number of recipients. They do not expose private keys, the plaintext archive key, archive paths, or plaintext contents.

## RGX identity files

`v0.5.0-alpha1` introduces dedicated RGX identities. Existing arbitrary SSH private keys are not reused.

A v2 public identity contains:

- an X25519 public key for recipient archive-key wrapping;
- an Ed25519 public key for detached archive signatures;
- a short RGX Key-ID.

The Ed25519 signing seed is derived from the private RGX root secret using a dedicated BLAKE3 derive-key context. This provides domain separation between recipient encryption and signing material, but the overall v0.5 key construction remains experimental and should receive independent review before a stable format is declared.

### Unprotected private identity

The default `rgx keygen` mode stores private key material in an RGX key file so automatic recipient unlock can happen without an additional prompt. On Unix the file is created with mode `0600`.

This mode depends on operating-system account and filesystem protection. A process or attacker that can read the private key file can decrypt archives addressed to that key and can derive its signing key.

### Passphrase-protected private identity

`rgx keygen --protect` encrypts the private key material at rest with XChaCha20-Poly1305. The encryption key is derived from a passphrase with Argon2id using the current 64 MiB / 3 iteration / 1 lane profile. Fresh salt and nonce values are generated for every protected key file, and key metadata is included as authenticated associated data.

Protected identities require the key passphrase before use. Recipient operations can receive it through `RGX_KEY_PASSWORD`; the `sign` command can also use `--key-password-env NAME`.

Environment variables may be observable to privileged local processes or captured by CI configuration. Treat them as secrets.

Windows-specific ACL hardening, OS keychain integration, TPMs, smart cards, and hardware tokens are not implemented yet.

## Detached signatures

`v0.5.0-alpha1` can create detached Ed25519 signature files without modifying the `.rgx` archive.

The signed message is domain-separated and contains:

- the exact archive byte length;
- the BLAKE3 digest of the exact archive bytes.

Verification recomputes the archive digest and checks the Ed25519 signature using the trusted RGX public key supplied by the verifier. Signature files use a strict canonical parser that rejects reordered/missing fields, malformed values, unexpected trailing data, and non-canonical whitespace/newline changes.

A successful `rgx verify` establishes structural and cryptographic integrity of the archive. A successful `rgx verify-signature` additionally establishes that the exact archive bytes were signed by the holder of the corresponding RGX private identity. It does **not** prove the real-world identity of that key holder unless the public key has been authenticated through a trusted process.

## Authentication and corruption handling

Encrypted readers reject malformed or unauthenticated data, including incorrect passwords or keys, modified ciphertext, modified authenticated metadata, frame reordering, missing/truncated frames, unexpected trailing data, invalid recipient-envelope lengths/counts, and unsupported or unreasonable KDF/frame parameters.

After decryption, the normal RGX parser additionally validates BLAKE3 chunk hashes, reconstructed-file hashes, paths, chunk references, deduplication relationships, and footer statistics.

The reference reader/extractor also rejects dangerous inner structures such as absolute or parent-traversal paths, duplicate or ambiguous paths, file paths reused as parent directories, unsupported or oversized chunk declarations, unknown/forward chunk references, duplicate/unreferenced chunk records, inconsistent footer statistics, and trailing bytes after the inner footer.

The writer refuses to create the final archive inside the directory tree being packed.

## Metadata exposure

Password Private Mode hides the complete inner RGX structure but reveals outer cryptographic parameters and approximate encrypted size.

Recipient Mode additionally reveals:

- that the archive is recipient protected;
- the `RGXR` envelope version;
- the number of recipient slots;
- stable short recipient Key-IDs;
- whether a password fallback exists;
- public password-KDF parameters when fallback is enabled;
- approximate total archive size.

Stable Key-IDs can allow correlation of the same recipient public key across multiple archives. This is a known privacy trade-off in the current experimental format.

## Plaintext handling

Both encrypted modes stream packing directly into authenticated encryption and decrypt frames on demand through seekable readers. They do not intentionally materialize a complete plaintext inner archive in a temporary file.

Requested plaintext chunks necessarily exist briefly in process memory while being verified or extracted. RGX cannot protect plaintext from a compromised host, process-memory inspection, swap, or the destination files intentionally written during extraction.

## Password handling

Archive passwords and protected-key passphrases are not accepted as literal secret command-line arguments. Interactive prompts do not echo secrets. Automation may use explicitly named environment variables where supported.

Password fallback is optional. Enabling it means archive confidentiality also depends on the entropy and handling of that fallback password, not only on recipient private-key security.

## Randomness

Archive keys, X25519 ephemeral private keys, salts, and nonce prefixes are generated from the operating system cryptographically secure random-number generator. Fixed byte strings used for AEAD associated data and BLAKE3 derive-key contexts are public domain-separation labels, not secret cryptographic keys.

## Validation

Continuous validation includes cross-platform unit/integration tests, Clippy/format checks, Rust 1.88 MSRV checking, dependency audits, static Linux builds, CodeQL, end-to-end encryption/key/signature/tamper scenarios, and parser fuzzing. Current fuzz targets include both the plain archive parser and the `RGXR` recipient-envelope parser.

Release-candidate validation treats CodeQL findings and dependency-audit failures as blockers until they are fixed or individually reviewed and shown to be non-security-impacting. Test fixtures avoid embedding production credentials or key material.

These tests improve confidence in implementation correctness and parser robustness; they are not a substitute for independent security review, formal analysis, or a professional cryptographic audit.

## Supported versions

Only the newest published alpha receives security fixes. Pre-1.0 archives and identity formats may require migration when experimental formats change.

## Reporting vulnerabilities

Do not open a public issue or pull request containing exploit details. Use GitHub's private vulnerability reporting feature under the Security tab when available.

Include the affected version, platform, reproduction steps, impact, and any suggested mitigation. Maintainers will acknowledge a report as soon as practical, coordinate validation privately, and publish an advisory together with a fixed version when appropriate. No response-time or bounty commitment is currently offered.
