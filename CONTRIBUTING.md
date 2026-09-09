# Contributing to RGX

RGX is an experimental archive format and security-sensitive parser. Small, reviewable changes with tests are preferred.

## Development

Use a stable Rust toolchain and run:

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
```

Changes to parsing, paths, private envelopes, authentication, or extraction must include negative tests for malformed input.

## Pull requests

- Base feature work on the `test` branch.
- Keep `main` release-ready.
- Explain format and compatibility effects.
- Update `docs/FORMAT.md` for wire-format changes.
- Never include real secrets or sensitive archives in fixtures.

## Security reports

Do not open public issues for suspected vulnerabilities. Follow [SECURITY.md](SECURITY.md).
