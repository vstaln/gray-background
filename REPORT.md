# Rust conversion and installer verification

## Implemented

- Standalone Rust `background` binary in this separate repository; Python runtime
  and launcher retired to ignored local artifacts.
- Preserved opacity, precomputed vertical fades, image limits, host requests,
  deferred manifest requests, shutdown/EOF/SIGTERM cleanup, bounded frames,
  real 20-second host timeout and local errors.
- `gray install plugin background` downloads a platform binary, checks its SHA-256
  and manifest, and registers it globally in the existing sidecar lockfile.
- Fixed Gray's shared plugin loader suppressing default bash tools when only a
  globally installed sidecar was present and no project profile existed.
- Added release workflow for Linux/macOS x86_64/aarch64 binaries and checksums.

## Evidence

- 10 Rust plugin tests pass (3 image tests, 7 wire/process tests).
- Native Linux x86_64 musl release built; `file` reports static PIE, stripped;
  artifact is approximately 1.7 MiB.
- Gray plugin conformance and the real-TUI PTY integration use the Rust binary.
- Installer integration invokes the real `gray install plugin background` command
  against local release assets, with PATH pointing to a nonexistent directory.
  Checks global discovery without YAML, default bash retained, real gradient PNG
  from installed executable, repeat install, checksum mismatch, invalid executable,
  old file/registry preservation, and re-enabling a disabled plugin.
- Installer unit tests cover platform selection, checksum format and publish
  rollback on registry write failure. Gray's complete serial crate tests and
  gray-plugin's complete crate tests pass.

## Not shipped yet

No public remote/release exists. The default GitHub asset URLs cannot serve the
binary until publication is approved. Local release-mirror tests are not public
installation. The cross-platform release workflow has not run. Installed Gray was
not replaced during this task. GPU rendering in Ghostty is still not manually
verified; no new visual-success claim is made.
