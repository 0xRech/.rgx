# RGX for Windows

This directory contains the Windows packaging, Explorer integration, installer, and update checker for RGX.

## Windows installer

Release builds can produce a normal per-user installer named similar to:

```text
RGX-Setup-0.5.0-alpha2-windows-x86_64.exe
```

The installer:

- installs `rgx.exe` to `%LOCALAPPDATA%\Programs\RGX`;
- adds that directory to the current user's `PATH`;
- registers `.rgx` as **RGX Archive**;
- installs the transparent RGX file icon;
- adds **Open with RGX**, **Extract with RGX**, and **Verify with RGX** Explorer actions;
- installs `rgx-update` for checking GitHub Releases;
- creates an uninstall entry and Start Menu shortcuts;
- requires no administrator privileges.

The installer uses a fixed Inno Setup `AppId`. Running a newer RGX installer upgrades the existing installation in place instead of creating a second copy.

## Updates

After installer-based installation, run:

```powershell
rgx-update
```

RGX compares the installed version with GitHub Releases. Alpha installations automatically include prereleases in the update channel. Stable installations only see stable releases unless you explicitly run:

```powershell
rgx-update -IncludePrerelease
```

To check without opening the release page:

```powershell
rgx-update -CheckOnly
```

When an update is available, the checker offers to open the official GitHub Release page. Download the newer `RGX-Setup-...exe` from that release and run it; the installer performs an in-place upgrade and refreshes the Explorer integration.

Release assets also include `SHA256SUMS` so downloaded installers and binaries can be independently verified.

## Explorer integration

The Explorer registration is implemented by `register-rgx.ps1` and uses `HKCU\Software\Classes`, so administrator privileges are not required.

It adds:

- `.rgx` as **RGX Archive**;
- the official multi-size RGX `.ico` file;
- double-click/Open via `rgx info`;
- **Open with RGX**;
- **Extract with RGX**;
- **Verify with RGX**.

The shell helper and icon are copied to `%LOCALAPPDATA%\RGX\Shell`, so Explorer does not depend on the repository directory after registration.

## Build the installer locally

Build RGX first:

```powershell
cargo build --release --locked
```

Install Inno Setup 6 and then set the two build variables:

```powershell
$env:RGX_VERSION = "0.5.0-alpha1"
$env:RGX_BINARY = (Resolve-Path ".\target\release\rgx.exe").Path
iscc .\packaging\windows\rgx.iss
```

The resulting setup executable is written to:

```text
packaging\windows\dist\
```

## Register Explorer integration without the installer

```powershell
powershell -ExecutionPolicy Bypass -File .\packaging\windows\register-rgx.ps1 -RgxExe .\target\release\rgx.exe
```

After Explorer refreshes, `.rgx` files should display the RGX icon. Test double-click and the context-menu commands on a non-critical archive.

## Remove the Explorer integration

```powershell
powershell -ExecutionPolicy Bypass -File .\packaging\windows\register-rgx.ps1 -Unregister
```

The unregister path removes only RGX-owned file associations, context-menu verbs, Open-With registration, and local shell helper/icon state.

## Current limitation

RGX is still a CLI application rather than an archive-browser GUI. The default **Open** action therefore displays `rgx info` in a console window. A future desktop/browser UI can replace that action without changing the `.rgx` file type registration.
