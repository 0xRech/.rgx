# RGX Windows shell integration

This directory contains the experimental Windows Explorer integration for the `test` branch.

## What it adds

- `.rgx` is registered as **RGX Archive** for the current Windows user.
- RGX archives use the official RGX file icon.
- Double-clicking a `.rgx` file runs `rgx info` in a console window.
- Explorer context menu actions are added for:
  - **Open with RGX**
  - **Extract with RGX**
  - **Verify with RGX**
- Registration uses `HKCU\Software\Classes`, so administrator privileges are not required.

The shell helper and generated `.ico` file are copied to `%LOCALAPPDATA%\RGX\Shell` so the Explorer integration does not depend on the repository location after registration.

## Test it locally

Build RGX first:

```powershell
cargo build --release --locked
```

Register the file type:

```powershell
powershell -ExecutionPolicy Bypass -File .\packaging\windows\register-rgx.ps1 -RgxExe .\target\release\rgx.exe
```

After Explorer refreshes, `.rgx` files should display the RGX icon. You can then test double-click and the context menu commands on a non-critical test archive.

## Remove the integration

```powershell
powershell -ExecutionPolicy Bypass -File .\packaging\windows\register-rgx.ps1 -Unregister
```

The uninstall path removes only the RGX ProgID, RGX-specific context menu verbs, RGX Open-With registration, and the local shell helper/icon state.

## Current limitation

RGX is currently a CLI application rather than an archive-browser GUI. For that reason, the default **Open** action displays `rgx info` and keeps the console open. A future desktop/browser UI can replace that action without changing the `.rgx` file type registration.
