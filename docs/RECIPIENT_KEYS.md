# RGX recipient keys (experimental)

Recipient-key support is currently implemented on the `test` branch and is not part of the stable alpha format yet.

## Design

RGX uses a hybrid design rather than encrypting archive data directly with a public-key algorithm:

1. RGX creates a random 256-bit archive key.
2. The normal RGX payload is encrypted with XChaCha20-Poly1305.
3. The archive key is wrapped independently for each X25519 recipient.
4. An optional password-fallback slot wraps the same archive key using Argon2id + XChaCha20-Poly1305.
5. On read, RGX first searches for a matching local RGX identity. Only if no matching key exists does it request the fallback password.

The private key never becomes part of the `.rgx` archive.

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

The experimental recipient envelope starts with `RGXR` and contains only recipient-access metadata followed by an authenticated encrypted RGX payload:

```text
RGXR envelope v1
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
└── encrypted RGX payload
```

Recipient slots reveal short key identifiers and the number of recipients. They do not contain private keys, plaintext archive contents, file names, paths, or the unwrapped archive key.

## Current test-branch limitations

- Recipient keys are dedicated RGX X25519 keys, not existing SSH keys.
- The private key file is stored as RGX key material. Unix creation uses mode `0600`; OS keychain/hardware-token storage is not implemented yet.
- The test implementation currently reuses the existing private RGX payload layer internally, so recipient operations have some extra temporary encrypted-file I/O and Argon2id overhead. This can be removed when the recipient envelope becomes a native streaming layer.
- The format is experimental and may change before it is merged into `main`.
- Independent cryptographic review has not yet been performed.
