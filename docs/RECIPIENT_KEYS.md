# RGX recipient keys (experimental)

Recipient-key support is currently implemented on the `test` branch and is not part of the stable alpha format yet.

## Design

RGX uses a hybrid design rather than encrypting archive data directly with a public-key algorithm:

1. RGX creates a random 256-bit archive key.
2. The RGX archive stream is encrypted directly with XChaCha20-Poly1305 using that archive key.
3. The archive key is wrapped independently for each X25519 recipient.
4. An optional password-fallback slot wraps the same archive key using Argon2id + XChaCha20-Poly1305.
5. On read, RGX first searches for a matching local RGX identity. Only if no matching key exists does it request the fallback password.

The private key never becomes part of the `.rgx` archive.

The recipient payload no longer passes through the password-based Private Mode. Argon2id is therefore not used for normal recipient payload encryption; it is used only when a password-fallback slot is requested.

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

The BLAKE3 hash of the complete `RGXR` recipient envelope is stored inside the authenticated payload header. This binds the outer recipient/password metadata to the encrypted payload: changing the recipient envelope causes payload verification to fail.

The keyed reader implements `Read + Seek`, so `list`, `find`, `cat`, selective extraction and verification can operate through the encrypted stream without first writing a complete temporary decrypted or re-encrypted archive.

This removes the previous test implementation's extra payload Argon2id derivation, temporary inner `.rgx` archive and full-file copy.

## Create an RGX identity

```bash
rgx keygen
```

The default files are:

```text
~/.ssh/id_rgx
~/.ssh/id_rgx.pub
```

A custom location is possible:

```bash
rgx keygen --output ./keys/alice_id_rgx
```

The public key may be distributed to senders. Keep the private key secret.

## Create a recipient-protected archive

For one recipient:

```bash
rgx pack ./data data.rgx --recipient ~/.ssh/alice_id_rgx.pub
```

For multiple recipients:

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

For automation, the fallback password can be supplied through an environment variable:

```bash
RGX_PASSWORD='fallback passphrase' \
  rgx pack ./data data.rgx \
  --recipient alice_id_rgx.pub \
  --password-fallback \
  --password-env RGX_PASSWORD
```

## Automatic unlock

For recipient-protected archives, the CLI searches the standard RGX identity locations, including:

```text
~/.ssh/id_rgx
~/.config/rgx/keys/id_rgx
```

On Windows it also checks:

```text
%APPDATA%\RGX\keys\id_rgx
```

If the matching key is found, normal commands do not require an archive password:

```bash
rgx list data.rgx
rgx verify data.rgx
rgx extract data.rgx ./restore
```

An explicit identity can be selected with:

```bash
rgx extract data.rgx ./restore --identity ./keys/alice_id_rgx
```

If no matching identity is found and the archive contains a password-fallback slot, RGX prompts for that password. If no fallback exists, access is refused.

## Envelope layout

The experimental recipient envelope starts with `RGXR` and contains recipient-access metadata followed by the native authenticated `RGXK` payload:

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

Recipient slots reveal short key identifiers and the number of recipients. They do not contain private keys, plaintext archive contents, file names, paths, or the unwrapped archive key.

## Current test-branch limitations

- Recipient keys are dedicated RGX X25519 keys, not existing SSH keys.
- The private key file is currently stored as RGX key material. Unix creation uses mode `0600`; OS keychain, TPM and hardware-token storage are not implemented yet.
- `RGXR` v2 is experimental and may change before it is merged into `main`.
- Compatibility with the short-lived test-only `RGXR` v1 envelope is not guaranteed.
- Independent cryptographic review has not yet been performed.
- Performance improvements from native streaming should be measured with repeated benchmark runs before publishing speed claims.
