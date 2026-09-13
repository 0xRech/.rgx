# RGX GitHub Packages

RGX publishes an OCI container package to the GitHub Container Registry (GHCR).

## Package

```text
ghcr.io/0xrech/rgx
```

The container contains the same `rgx` CLI as the native release builds.

## Tags

- `edge` — current `main` build.
- `vX.Y.Z...` — exact Git tag, for example `v0.5.0-alpha1`.
- `X.Y.Z...` — semantic version generated from a release tag.
- `X.Y` — stable minor alias when applicable.
- `latest` — generated automatically for stable semantic releases; prereleases do not replace it.
- `sha-...` — immutable commit-specific image tag.

## Pull the package

```bash
docker pull ghcr.io/0xrech/rgx:edge
```

For a versioned release:

```bash
docker pull ghcr.io/0xrech/rgx:0.5.0-alpha1
```

## Run RGX

```bash
docker run --rm ghcr.io/0xrech/rgx:edge --version
```

The image uses `/data` as its working directory. Mount a local directory there when working with archives:

```bash
docker run --rm -it \
  -v "$PWD:/data" \
  ghcr.io/0xrech/rgx:edge \
  pack /data/project /data/project.rgx
```

PowerShell:

```powershell
docker run --rm -it `
  -v "${PWD}:/data" `
  ghcr.io/0xrech/rgx:edge `
  pack /data/project /data/project.rgx
```

## RGX identities in the container

The image defines `HOME=/root`, so automatic identity discovery checks `/root/.ssh/id_rgx` inside the container. Mount an existing RGX identity when needed:

```bash
docker run --rm -it \
  -v "$PWD:/data" \
  -v "$HOME/.ssh:/root/.ssh:ro" \
  ghcr.io/0xrech/rgx:edge \
  verify /data/archive.rgx
```

Use a writable `.ssh` mount if you intentionally run `rgx keygen` inside the container.

## Publishing

`.github/workflows/packages.yml` publishes the package automatically:

- after pushes to `main`, producing `edge` and commit tags;
- after Git tags matching `v*`, producing versioned tags;
- manually through `workflow_dispatch`.

Publishing uses the repository-provided `GITHUB_TOKEN` with `packages: write`. No personal access token is required for the normal repository workflow.

## Native downloads

The GHCR package complements, rather than replaces, the native binaries and Windows installer published through GitHub Releases.
