# Gray Background

A **standalone Rust binary** plugin. No Python, virtual environment, Cargo, or
project YAML is needed on user machines. Gray still owns terminal rendering;
this plugin prepares the PNG once and sends `host/background` over NDJSON.

## Install

With the updated Gray build and published v0.1.0 assets:

```sh
gray install plugin background
```

Restart Gray, then:

```text
/background /absolute/path/image.png 0.4 top-to-bottom
/background /absolute/path/image.png 0.4 bottom-to-top
/background /absolute/path/image.png 0.4 none
/background off
```

`none` is the default; opacity defaults to 0.2. The direction names mean fade
from chosen opacity to transparent in that direction. Original image alpha is
preserved. A one-row image uses uniform opacity.

**Publication status:** source is public at `vstaln/gray-background`. The `v0.1.0`
release assets are not published yet, so the default public command cannot
download until that release exists and the updated Gray binary is installed.
Do not mistake local mirror tests for a published release.

## How installation works

- Gray selects Linux/macOS x86_64/aarch64, downloads a prebuilt binary plus its
  SHA-256 sidecar, verifies the hash and plugin manifest, and installs it under
  `$GRAY_HOME/plugins/background/background` (default `$HOME/.gray`).
- Registration in the existing user plugin lock makes `/background` available
  from every project; no `gray.yml` entry or shell startup edits.
- Repeat installs are a no-op for the registered, enabled version with an existing
  binary. A disabled installation is reinstalled/enabled. Failures before commit
  leave the old installation intact; registry-save failure rolls back the files.
- The checksum comes from the same HTTPS release origin; it checks integrity, not
  an independent signature. As with other plugins, installed code runs with your
  user privileges.
- Discord's Python-backed CLI installer is unchanged. Background is a native
  sidecar in the user plugin lock, not a `gray background setup` CLI command.

## Runtime

Image preparation supports PNG, JPEG, GIF (first frame), and WebP. Opacity and
linear gradient are baked once into a prepared PNG. No per-frame gradient math or
plugin draw loop. The terminal still performs normal alpha blending.
Source limits: 32 MiB and 16 million pixels; output thumbnail at most 1920×1080,
prepared PNG at most 8 MiB. Host loads bytes before acknowledging, after which the
private temp directory is deleted. EOF, shutdown, SIGTERM, and timeout clean it up.

The existing Gray rendering hook handles draw/resize placement, modal suspension,
restoration and normal exit cleanup. Ghostty/Kitty only; no tmux/screen support.
Paths with spaces are not supported by Gray's current slash-command tokenizer.
A model must be configured for Gray's lazy plugin dispatch; background commands
make no provider call. SIGKILL/hard crashes cannot guarantee cleanup.

**Visual status:** packet/lifecycle integration is tested with the real Gray TUI
in a PTY. Ghostty GPU layering/readability still needs manual confirmation.

## Development

```sh
cd ~/grayplugins/gray-background
CARGO_BUILD_JOBS=4 cargo test --locked
CARGO_BUILD_JOBS=4 cargo build --release --locked
cargo fmt --check
<gray-checkout>/target/debug/gray plugin check ~/grayplugins/gray-background/target/release/background
```

`test_install.py` and `test_host.py` are development-only integration tests (Python
and Pillow are test dependencies, not plugin/runtime dependencies):

```sh
GRAY_BACKGROUND_TEST_BINARY=<gray-checkout>/target/debug/gray python3 -m unittest -v test_install test_host
```

For release tests, `GRAY_BACKGROUND_TEST_PLUGIN` selects a built native artifact.
The installer supports explicit `GRAY_BACKGROUND_RELEASE_BASE` for release mirrors
and local validation. HTTPS is required, except loopback HTTP. No source-build
fallback is performed. The installation test uses a temporary home, a local HTTP
release server, and an unusable PATH to prove Python/Cargo are not invoked.

## Release

The tag workflow builds these assets and corresponding `.sha256` files:

- `background-x86_64-unknown-linux-musl`
- `background-aarch64-unknown-linux-musl`
- `background-x86_64-apple-darwin`
- `background-aarch64-apple-darwin`

All matrix builds must succeed before publication. Tag must match Cargo version.
The Gray catalog pins v0.1.0 at `vstaln/gray-background`; publishing/new tags and
Gray's catalog/version changes are explicit release operations, not install-time
build steps. Cross-platform workflow execution has not yet occurred.
