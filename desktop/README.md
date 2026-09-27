# RGX Desktop

RGX Desktop is the native Tauri 2 interface for the existing RGX Rust core.

The desktop application deliberately does **not** implement a second archive engine. Packing,
extraction, verification, Private Mode, Recipient Mode and identities call the same
`rgx` crate modules used by the CLI.

## Current alpha scope

- Pack files or folders as Plain RGX.
- Pack with Private Mode (Argon2id + XChaCha20-Poly1305).
- Pack for one or more X25519 RGX recipients.
- Optional recipient password fallback.
- Extract complete archives or one selected archive path.
- Automatic recipient identity discovery.
- Explicit identity selection, including protected identities.
- Archive information and integrity verification.
- Content listing (UI display is capped at 1,000 entries).
- Generate passwordless or passphrase-protected RGX identities.
- Native open/save/folder dialogs.
- Windows, macOS and Linux Tauri build configuration.

## Security design

- All file processing is local.
- The UI loads only bundled local assets; no CDN or remote web application is used.
- Tauri CSP is enabled and `Object.prototype` is frozen.
- Only the native dialog plugin is granted to the main webview.
- Archive operations are registered Tauri commands and use the existing Rust implementation.
- Password strings are moved into `Zeroizing<String>` in the backend operation as early as possible.
- Existing archives and extraction directories are not silently overwritten.
- The existing Windows RGX shell association remains authoritative until Desktop supports
  opening an archive path directly from the operating system.

RGX is still pre-1.0 and unaudited. The desktop interface does not change that security status.

## Run locally

Install the Tauri 2 prerequisites for your operating system:

https://v2.tauri.app/start/prerequisites/

Install the Tauri CLI:

```bash
cargo install tauri-cli --version "^2"
```

From the repository root:

```bash
cd desktop
cargo tauri dev
```

No Node.js package manager is required. The frontend is plain HTML, CSS and JavaScript.

## Build bundles

```bash
cd desktop
cargo tauri build
```

The configured Tauri bundler can create the native targets supported by the current host
(NSIS/MSI on Windows, app/DMG on macOS, and Linux package formats on Linux).

## Layout

```text
desktop/
  ui/
    index.html
    styles.css
    app.js
  src-tauri/
    Cargo.toml
    build.rs
    tauri.conf.json
    capabilities/
      default.json
    src/
      main.rs
```
