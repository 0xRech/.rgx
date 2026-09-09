## Summary

## Validation

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --locked --all-targets --all-features -- -D warnings`
- [ ] `cargo test --locked --all-features`

## Compatibility and security

- [ ] No wire-format change
- [ ] Format documentation updated when required
- [ ] Negative tests added for malformed or hostile input
- [ ] No secrets or sensitive fixtures included
