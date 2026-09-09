# Public alpha release checklist

## Before making the repository public

- [ ] Scan every branch and the complete Git history for secrets.
- [ ] Revoke any credential that has ever been committed.
- [ ] Confirm third-party assets and documentation may be published.
- [ ] Confirm the project name and branding may be used publicly.
- [ ] Enable branch protection for `main`.
- [ ] Require CI, Security, and Fuzz checks before merging.
- [ ] Enable private vulnerability reporting after the repository is public.
- [ ] Enable GitHub secret scanning, push protection, Dependabot, and code scanning.
- [ ] Review repository collaborators and deploy keys.

## Alpha validation

- [ ] All CI platforms pass.
- [ ] Dependency audit and license checks pass.
- [ ] Scheduled fuzz smoke test passes.
- [ ] v0.3 compatibility test passes.
- [ ] Test release installation on clean Linux, Windows, and macOS systems.
- [ ] Verify every SHA-256 checksum.
- [ ] Confirm README and SECURITY warnings are visible.
- [ ] Confirm `Cargo.toml` and the `rgx` package entry in `Cargo.lock` use the same version.
- [ ] Confirm the README version badge and public-alpha warning match `Cargo.toml`.

## Release

- [ ] Merge the reviewed `test` pull request into `main`.
- [ ] Create a signed tag matching `Cargo.toml` (current source: `v0.5.0-alpha1`).
- [ ] Inspect generated release binaries before publishing.
- [ ] Confirm the GitHub Release tag/name and packaged binaries match the source version.
- [ ] Keep the release marked as a prerelease.
