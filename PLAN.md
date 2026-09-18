# Rust plugin and native catalog installation

Approved: separate Rust plugin; one-command global installation; no runtime
Python/Cargo/YAML dependency.

- [x] Port image preparation and bounded NDJSON host exchange to Rust.
- [x] Build/test a statically linked Linux release binary.
- [x] Add platform release workflow and checksums.
- [x] Reuse Gray download verification, manifest checks and global sidecar lock.
- [x] Verify repeated installs, bad downloads, wrong binaries and rollback.
- [x] Verify real Gray installation/autoload and native-plugin TUI integration.
- [ ] Publish repo and v0.1.0 release after user approval.
- [ ] Release/install the updated Gray binary; manual GPU visual confirmation.

Public installation is blocked only on publication/distribution. Local validation
uses an explicit release-base override, never a hidden source-build fallback.
