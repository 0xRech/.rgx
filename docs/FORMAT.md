# RGX Format Specification — Draft 0.5

This document describes the formats used by RGX `v0.5.0-alpha1`. RGX remains pre-1.0 and experimental format revisions may still be intentionally incompatible.

All multi-byte integers are little-endian unless explicitly stated otherwise.

RGX currently has three archive representations:

1. plain `RGX\0` — the RGX v0.2 inner archive directly;
2. password Private Mode `RGXE` — an authenticated encrypted stream containing one RGX v0.2 inner archive;
3. recipient Mode `RGXR` — recipient/password key slots followed by a native authenticated `RGXK` stream containing one RGX v0.2 inner archive.

Detached `RGX-SIGNATURE-1` files authenticate the exact bytes of any of these archive representations without changing the archive itself.

# 1. RGX v0.2 inner archive

## Archive header

| Offset | Size | Field | Value / meaning |
| --- | ---: | --- | --- |
| 0 | 4 | Magic | `52 47 58 00` (`RGX\0`) |
| 4 | 2 | Major version | `0` |
| 6 | 2 | Minor version | `2` |
| 8 | 4 | Flags | `0` in v0.2 |
| 12 | 4 | Reserved | `0` |

The header is followed by chunk, file, and directory records and ends with exactly one footer.

## Chunk record

A unique content chunk is stored as a `CHNK` record.

| Field | Size | Meaning |
| --- | ---: | --- |
| Magic | 4 | ASCII `CHNK` |
| Compression | 1 | `0` store, `1` Zstandard |
| Reserved | 3 | `0` |
| Original size | 8 | Uncompressed chunk size |
| Payload size | 8 | Stored payload size |
| BLAKE3 | 32 | Digest of the uncompressed chunk |
| Payload | variable | Raw or Zstandard-compressed bytes |

The reference writer emits non-empty chunks up to 1 MiB. A stored chunk has `payload size == original size`. A Zstandard chunk is emitted only when compression reduces size. A BLAKE3 chunk identifier may be defined by a `CHNK` record only once.

## File record

| Field | Size | Meaning |
| --- | ---: | --- |
| Magic | 4 | ASCII `FILE` |
| Path length | 4 | UTF-8 path length in bytes |
| Chunk count | 4 | Number of 32-byte chunk references |
| Original size | 8 | Reconstructed file size |
| BLAKE3 | 32 | Digest of the fully reconstructed file |
| Path | variable | UTF-8 relative path |
| Chunk references | `32 × count` | Ordered BLAKE3 chunk identifiers |

Chunk references resolve to chunks defined earlier in the archive. Empty files contain no chunk references and use the BLAKE3 digest of an empty byte string.

## Directory record

| Field | Size | Meaning |
| --- | ---: | --- |
| Magic | 4 | ASCII `DIRE` |
| Path length | 4 | UTF-8 path length in bytes |
| Reserved | 4 | `0` |
| Path | variable | UTF-8 relative path |

## Inner footer

| Field | Size | Meaning |
| --- | ---: | --- |
| Magic | 4 | ASCII `RGXF` |
| Entries | 8 | File + directory count |
| Files | 8 | File count |
| Directories | 8 | Directory count |
| Unique chunks | 8 | Number of `CHNK` records |
| Chunk references | 8 | Total references across files |
| Original bytes | 8 | Sum of logical file sizes |
| Stored payload bytes | 8 | Sum of physical chunk payload sizes |
| Deduplicated bytes | 8 | Logical bytes eliminated by chunk reuse |

Readers recompute and validate these values. Missing/multiple footers, mismatched statistics, unreferenced chunks, or trailing bytes are corruption.

## Chunking and compression

The reference writer currently uses content-defined chunking with a 64-byte rolling window, 64 KiB minimum, approximately 256 KiB target, and 1 MiB maximum chunk size. Chunk boundaries are a writer implementation choice rather than a container compatibility requirement.

Each unique chunk is independently tried with Zstandard. It is stored raw if the compressed representation would not be smaller.

## Integrity and path safety

Per-chunk and per-file BLAKE3 digests detect corruption and reconstruction errors. They are not by themselves sender authentication for a plain archive.

Readers reject unsafe or ambiguous paths, including absolute paths, empty components, `.` / `..`, backslash ambiguity, duplicate paths, and file paths reused as parent directories. The reference extractor writes into a new destination and does not silently overwrite an existing extraction tree.

# 2. Password Private envelope — `RGXE` v0.4

A password-protected RGX archive begins with `RGXE`. Decrypting and concatenating its authenticated frame plaintext yields exactly one RGX v0.2 inner archive.

## Header

The header is 60 bytes.

| Offset | Size | Field | Value / meaning |
| --- | ---: | --- | --- |
| 0 | 4 | Magic | ASCII `RGXE` |
| 4 | 2 | Major | `0` |
| 6 | 2 | Minor | `4` for current writer |
| 8 | 1 | KDF | `1` = Argon2id |
| 9 | 1 | AEAD | `1` = XChaCha20-Poly1305 |
| 10 | 2 | Reserved | `0` |
| 12 | 4 | Argon2 memory | KiB |
| 16 | 4 | Argon2 iterations | count |
| 20 | 4 | Argon2 lanes | parallelism |
| 24 | 4 | Frame size | plaintext frame size |
| 28 | 16 | Salt | random per archive |
| 44 | 16 | Nonce prefix | random per archive |

Current writer profile:

```text
Argon2id memory:   65536 KiB
Argon2 iterations: 3
Argon2 lanes:      1
Frame size:        1048576 bytes
```

The password-derived key is 32 bytes and is used directly as the XChaCha20-Poly1305 key.

## Frame

| Offset | Size | Field | Meaning |
| --- | ---: | --- | --- |
| 0 | 4 | Magic | ASCII `FRAM` |
| 4 | 1 | Final flag | `0` normal, `1` final |
| 5 | 3 | Reserved | `0` |
| 8 | 8 | Sequence | starts at `0`, increments by one |
| 16 | 4 | Plaintext length | bytes before encryption |
| 20 | 4 | Ciphertext length | plaintext + 16-byte tag |
| 24 | variable | Ciphertext | XChaCha20-Poly1305 output |

Nonce construction:

```text
nonce = 16-byte random prefix || sequence_le_u64
```

Associated data is the exact `RGXE` header followed by the current frame header. This authenticates the cryptographic parameters, nonce prefix, frame size, sequence, final flag, and declared frame lengths.

The reference implementation streams directly into encrypted frames and provides seekable authenticated reads. It does **not** intentionally create a complete plaintext temporary inner archive.

# 3. Recipient envelope — `RGXR` v2

Recipient Mode is the main new archive feature in `v0.5.0-alpha1`.

The outer layout is:

```text
RGXR v2 header
recipient slot 0
recipient slot 1
...
optional password-fallback slot
RGXK v1 keyed payload stream
```

## RGXR prefix

The fixed prefix is 16 bytes.

| Offset | Size | Field | Value / meaning |
| --- | ---: | --- | --- |
| 0 | 4 | Magic | ASCII `RGXR` |
| 4 | 2 | Version | `2` |
| 6 | 2 | Flags | bit 0 = password fallback present |
| 8 | 2 | Recipient count | `1..64` in current implementation |
| 10 | 2 | Reserved | `0` |
| 12 | 4 | Total RGXR header length | prefix + all slots |

Readers reject unknown flags, zero recipients, more than 64 recipients, reserved-bit misuse, inconsistent lengths, truncation, and integer-overflow conditions.

## X25519 recipient slot

Each recipient slot is exactly 120 bytes.

| Field | Size | Meaning |
| --- | ---: | --- |
| Key-ID | 16 | short identifier of recipient public key |
| Ephemeral X25519 public key | 32 | per-slot ephemeral public value |
| Wrap nonce | 24 | XChaCha20-Poly1305 nonce |
| Wrapped archive key | 48 | 32-byte archive key + 16-byte AEAD tag |

RGX generates a fresh 256-bit archive key for every recipient-protected archive. Each slot performs X25519 Diffie-Hellman with an ephemeral sender secret and the recipient public key. The 32-byte wrapping key is derived with BLAKE3 `derive_key` using the context `rgx recipient archive-key wrap v1` and material containing the shared secret, ephemeral public key, recipient public key, and Key-ID.

The archive key is authenticated-encrypted with XChaCha20-Poly1305. Slot AAD includes the recipient-wrap domain marker, Key-ID, and ephemeral public key.

The private recipient key is never stored in the archive.

## Optional password-fallback slot

When flag bit 0 is set, one 100-byte password slot follows all recipient slots.

| Field | Size | Meaning |
| --- | ---: | --- |
| Argon2 memory KiB | 4 | current writer: 65536 |
| Argon2 iterations | 4 | current writer: 3 |
| Argon2 lanes | 4 | current writer: 1 |
| Salt | 16 | random |
| Wrap nonce | 24 | random XChaCha nonce |
| Wrapped archive key | 48 | same 32-byte archive key + tag |

The fallback password derives a 32-byte wrapping key with Argon2id. XChaCha20-Poly1305 wraps the same archive key used by all recipient slots. Password-slot AAD binds the KDF parameters and salt.

A fallback is optional. If enabled, confidentiality also depends on fallback password strength.

# 4. Native keyed payload — `RGXK` v1

Immediately after the complete `RGXR` header, Recipient Mode stores a native seekable encrypted stream beginning with `RGXK`.

## RGXK header

The header is 64 bytes.

| Offset | Size | Field | Value / meaning |
| --- | ---: | --- | --- |
| 0 | 4 | Magic | ASCII `RGXK` |
| 4 | 2 | Version | `1` |
| 6 | 2 | Reserved | `0` |
| 8 | 4 | Frame size | current writer: 1 MiB |
| 12 | 16 | Nonce prefix | random per payload |
| 28 | 32 | RGXR envelope hash | BLAKE3 of exact outer RGXR header bytes |
| 60 | 4 | Reserved | `0` |

Readers accept frame sizes only in the configured defensive range of 64 KiB through 4 MiB.

The envelope-hash field cryptographically binds the outer recipient/password metadata to the encrypted stream because the entire `RGXK` header is authenticated as part of every frame's AAD.

## RGXK frame

Each frame begins with a 24-byte header.

| Offset | Size | Field | Meaning |
| --- | ---: | --- | --- |
| 0 | 4 | Magic | ASCII `KFRM` |
| 4 | 1 | Final flag | `0` or `1` |
| 5 | 3 | Reserved | `0` |
| 8 | 8 | Sequence | starts at `0`, increments by one |
| 16 | 4 | Plaintext length | bytes before encryption |
| 20 | 4 | Ciphertext length | plaintext + 16-byte tag |
| 24 | variable | Ciphertext | XChaCha20-Poly1305 output |

Nonce construction:

```text
nonce = 16-byte RGXK nonce prefix || sequence_le_u64
```

AAD:

```text
AAD = 64-byte RGXK header || 24-byte KFRM header
```

Non-final frames are exactly the configured plaintext frame size. The final frame may be shorter. Readers require exactly one authenticated final frame and reject missing, reordered, truncated, modified, or trailing data.

The random 256-bit archive key obtained from a valid X25519 recipient slot or password-fallback slot is the XChaCha20-Poly1305 payload key directly; no additional password KDF is applied during normal recipient decryption.

# 5. RGX identity files — v2

`v0.5.0-alpha1` uses dedicated RGX identity files. They are text containers for portability during the alpha phase.

## Public key file

Header:

```text
RGX-PUBLIC-KEY-2
```

Fields include:

```text
X25519-Public: <32-byte hex>
Ed25519-Public: <32-byte hex>
Key-ID: <16-byte hex>
```

X25519 is used for recipient archive-key wrapping. Ed25519 is used for detached signatures.

## Unprotected private key file

Header:

```text
RGX-PRIVATE-KEY-2
```

The file contains the 32-byte private RGX secret and its Key-ID. On Unix the reference writer creates it with mode `0600`.

This representation permits zero-prompt automatic recipient unlock but relies on host/filesystem protection.

## Protected private key file

Header:

```text
RGX-PRIVATE-KEY-2-ENCRYPTED
```

The private secret is encrypted with XChaCha20-Poly1305 under a 32-byte key derived from the passphrase with Argon2id. The current writer uses 64 MiB, 3 iterations, and 1 lane with a fresh 16-byte salt and 24-byte nonce.

Authenticated metadata includes a domain marker, Key-ID, X25519 public key, Argon2 parameters, and salt. Decryption additionally verifies that the recovered private secret reproduces the stored X25519 public key and Key-ID.

The v0.5 reader retains recipient-use support for the earlier `RGX-PRIVATE-KEY-1` / `RGX-PUBLIC-KEY-1` files. Legacy public files do not contain an Ed25519 verification key and therefore cannot verify v0.5 detached signatures.

# 6. Detached signature file — `RGX-SIGNATURE-1`

Detached signatures do not alter the `.rgx` archive.

The text signature file contains:

```text
RGX-SIGNATURE-1
Algorithm: Ed25519
Key-ID: <16-byte hex>
Archive-Length: <u64 decimal>
Archive-BLAKE3: <32-byte hex>
Signature: <64-byte hex>
```

The signed message is:

```text
"RGX-ARCHIVE-SIGNATURE-1" || archive_length_le_u64 || BLAKE3(exact archive bytes)
```

The Ed25519 signing seed is derived from the private RGX root secret using BLAKE3 `derive_key` with the context `rgx ed25519 signing seed v1`. The corresponding Ed25519 public key is stored in the v2 RGX public-key file.

`rgx verify-signature` recomputes the exact archive length and BLAKE3 digest before Ed25519 verification. A changed archive therefore fails before or during signature verification.

The signature-file parser is canonical and strict: it rejects missing, reordered, malformed, non-canonical, or trailing fields instead of silently ignoring additional data.

A valid signature authenticates possession of the corresponding RGX private key. Real-world identity trust depends on how the public key was obtained and authenticated.

# Compatibility policy

- Plain archives continue to use inner format v0.2.
- Current password Private writers use `RGXE` v0.4; the reader retains v0.3 compatibility.
- `v0.5.0-alpha1` recipient writers use `RGXR` v2 with `RGXK` v1 payloads.
- Short-lived test-only recipient formats are not guaranteed to remain compatible.
- RGX key files are v2 for new identities, with legacy v1 recipient-key loading retained where applicable.
- Detached signatures use `RGX-SIGNATURE-1`.
- Before RGX 1.0, format and key-file revisions may still be breaking.
