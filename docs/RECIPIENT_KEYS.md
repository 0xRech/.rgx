# RGX recipient keys — v0.5.0-alpha1

Recipient-key support is a primary cryptographic feature of RGX `v0.5.0-alpha1`. The format remains pre-1.0 and unaudited, so these interfaces and layouts can still evolve before a stable 1.0 specification.

## Design

RGX uses hybrid encryption rather than encrypting archive contents directly with a public-key primitive:

1. RGX creates a random 256-bit archive key.
2. The compressed RGX stream is encrypted with XChaCha20-Poly1305 using that archive key.
3. The archive key is wrapped independently for each X25519 recipient.
4. An optional password-fallback slot wraps the same archive key using Argon2id + XChaCha20-Poly1305.
5. On read, RGX searches for a matching local RGX identity first. Only if no matching identity is available does it use the password fallback.

The private key never becomes part of the `.rgx` archive.

## Native streaming payload

Recipient envelope version 2 writes the compressed RGX archive directly into an authenticated keyed stream:

```text
RGXR envelope v2
      │
      ├── X25519 recipient slots
      ├── optional Argon2id password-fallback slot
      │
      └── RGXK keyed payload stream
            ├── stream header
            ├── authenticated frame 0
            ├── authenticated frame 1
            └── ...
```

The `RGXK` stream uses the random archive key directly with XChaCha20-Poly1305. Frames use a random 16-byte nonce prefix plus an increasing 64-bit frame sequence, giving each frame a unique 24-byte XChaCha20 nonce.

The BLAKE3 hash of the complete `RGXR` recipient envelope is stored inside the authenticated payload header. This binds the outer recipient/password metadata to the encrypted payload: modifying recipient metadata causes payload verification to fail.

The keyed reader implements `Read + Seek`, so `list`, `find`, `cat`, selective extraction, and verification can operate through the encrypted stream without creating a complete plaintext temporary archive.

## Create an RGX identity

```bash
rgx keygen
```

Default paths:

```text
~/.ssh/id_rgx
~/.ssh/id_rgx.pub
```

A custom location is possible:

```bash
rgx keygen --output ./keys/alice_id_rgx
```

The v2 public key file contains two public keys:

- X25519 for recipient archive-key wrapping.
- Ed25519 for detached archive signatures.

The signing seed is domain-separated from the X25519 private secret with a dedicated BLAKE3 derive-key context. Existing arbitrary SSH keys are not reused.

## Private-key storage modes

### Passwordless identity

The default identity keeps the passwordless RGX recipient workflow: if the matching private key exists in a standard location, RGX can unlock the archive without an archive-password prompt.

On Unix, private-key creation uses mode `0600`. Protection of an unencrypted key therefore depends on the operating-system account and filesystem permissions.

### Passphrase-protected identity

To cryptographically protect private-key material at rest:

```bash
rgx keygen --protect
```

The private-key file is encrypted with XChaCha20-Poly1305. Its key is derived from the passphrase with Argon2id using the current profile of 64 MiB memory, 3 iterations, and 1 lane.

For non-interactive creation:

```bash
RGX_KEY_CREATE_PASSWORD='strong passphrase' \
  rgx keygen --protect --password-env RGX_KEY_CREATE_PASSWORD
```

For recipient commands using a protected identity, provide the passphrase in `RGX_KEY_PASSWORD`:

```bash
RGX_KEY_PASSWORD='strong passphrase' \
  rgx verify data.rgx --identity ~/.ssh/id_rgx
```

This key passphrase is separate from an archive password-fallback slot.

## Create a recipient-protected archive

One recipient:

```bash
rgx pack ./data data.rgx --recipient ~/.ssh/alice_id_rgx.pub
```

Multiple recipients:

```bash
rgx pack ./data data.rgx \
  --recipient alice_id_rgx.pub \
  --recipient bob_id_rgx.pub
```

Add a password fallback:

```bash
rgx pack ./data data.rgx \
  --recipient alice_id_rgx.pub \
  --password-fallback
```

For automation:

```bash
RGX_PASSWORD='fallback passphrase' \
  rgx pack ./data data.rgx \
  --recipient alice_id_rgx.pub \
  --password-fallback \
  --password-env RGX_PASSWORD
```

## Automatic unlock

RGX searches the standard identity locations:

```text
~/.ssh/id_rgx
~/.config/rgx/keys/id_rgx
```

On Windows it also checks:

```text
%APPDATA%\RGX\keys\id_rgx
```

With an unprotected matching identity, these commands do not require an archive password:

```bash
rgx list data.rgx
rgx verify data.rgx
rgx extract data.rgx ./restore
```

An explicit identity can be selected with:

```bash
rgx extract data.rgx ./restore --identity ./keys/alice_id_rgx
```

If no matching identity is found and the archive has a password-fallback slot, RGX requests that password. Without a matching key or fallback, access is refused.

## Detached Ed25519 signatures

The v0.5 identity can also sign an archive without modifying the archive container:

```bash
rgx sign data.rgx --identity ~/.ssh/id_rgx
rgx verify-signature data.rgx data.rgx.sig --public-key ~/.ssh/id_rgx.pub
```

The detached signature covers a domain-separated message containing the exact archive length and BLAKE3 digest. The `.sig` file stores:

- format marker `RGX-SIGNATURE-1`
- Ed25519 algorithm identifier
- RGX Key-ID
- archive byte length
- archive BLAKE3 digest
- Ed25519 signature

The signature parser is deliberately strict: malformed, reordered, duplicated/trailing, or non-canonical fields are rejected rather than silently ignored.

Because the signature is detached, the same mechanism works with plain, password-Private, and recipient-protected RGX archives.

`rgx verify` checks archive structure and cryptographic integrity. `rgx verify-signature` adds sender-key authenticity. It does not establish the real-world identity of the key holder by itself; the verifier must obtain and trust the public key through an appropriate channel.

## Envelope layout

```text
RGXR envelope v2
├── recipient slot A
│   ├── Key-ID
│   ├── ephemeral X25519 public key
│   ├── nonce
│   └── authenticated wrapped archive key
├── recipient slot B ...
├── optional password slot
│   ├── Argon2id parameters + salt
│   ├── nonce
│   └── authenticated wrapped archive key
└── RGXK payload
    ├── XChaCha20-Poly1305 stream header
    └── authenticated, seekable frames
```

Recipient slots reveal short stable key identifiers and the number of recipients. They do not contain private keys, plaintext archive contents, file names, paths, or the unwrapped archive key.

## Validation

Continuous validation includes unit/integration tests for recipient wrapping, password fallback, protected key-file roundtrips, wrong passphrases, automatic identity discovery, multiple recipients, payload tamper rejection, relative output paths, detached signatures, strict signature parsing, and signature tamper rejection.

Fuzzing exercises both the plain archive parser and the recipient-envelope parser. CI additionally runs on Linux, Windows, and macOS, checks Rust 1.88 compatibility, formatting/Clippy, dependency audits, and the static Linux release build.

## Current limitations

- `RGXR` v2 and the v0.5 key/signature formats remain experimental and may change before 1.0.
- Independent cryptographic review has not yet been performed.
- Unprotected keys are required for fully passwordless automatic unlock; protected keys need a passphrase supplied through `RGX_KEY_PASSWORD` for recipient operations.
- Windows-specific hardened ACL management, OS keychain integration, TPMs, smart cards, and hardware tokens are not implemented yet.
- Stable Key-IDs can correlate use of the same public key across archives.
- Password fallback is opt-in and makes archive confidentiality additionally depend on password strength.
