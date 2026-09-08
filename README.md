# runsite CLI

Command-line tool for the [RunSite](https://runsite.app) platform. Manage web
services, deployments and environment variables from your terminal or from CI.

## Install

**Linux / macOS:**

```bash
curl -fsSL https://raw.githubusercontent.com/runsite-platform/runsite-cli/master/install/install.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/runsite-platform/runsite-cli/master/install/install.ps1 | iex
```

The scripts detect your OS and architecture, download the matching binary from
GitHub Releases, verify its SHA-256 checksum, and install it
(`~/.local/bin/runsite` on Unix, `%LOCALAPPDATA%\runsite\bin\runsite.exe` on
Windows).

Pin a version with `RUNSITE_VERSION=v0.1.0` (or `$env:RUNSITE_VERSION` on
Windows). Override the install directory with `RUNSITE_INSTALL_DIR`.

Prebuilt targets: `x86_64`/`aarch64` Linux (musl, static), `x86_64`/`aarch64`
macOS, `x86_64` Windows.

## Authenticate

```bash
runsite login                          # email + password, mints an API key
runsite login --token ak_live_...      # use a key created in the dashboard
```

`--token` is the path for accounts that sign in with Google or GitHub and have
no password.

In CI, skip `login` entirely and set `RUNSITE_API_TOKEN=ak_live_...`.

## Commands

```
runsite login [--token ak_live_...]    # authenticate
runsite logout
runsite whoami

runsite service list
runsite service status [name|id]
runsite service start|stop|restart [name|id]

runsite deploy [service]               # trigger a deployment
runsite logs [service] [--tail 100]    # recent container logs

runsite env list [service]
runsite env set [service] KEY=VALUE KEY2=VALUE2
runsite env delete [service] KEY

runsite project list
runsite project use [name|id]

runsite context show
runsite context set-url https://api.runsite.app

runsite completions bash|zsh|fish
```

Every data command also accepts `--output json` for scripting.

The service argument is optional: with a single service (or a single service in
the selected project) the CLI resolves it automatically.

### Not in this release

`runsite shell`, `runsite run` and `runsite deploy --watch` need a live
WebSocket, which the public API does not expose to API keys yet. The commands
exist and tell you to use the dashboard instead.

## Configuration

Config lives at `~/.config/runsite/config.toml` (`%APPDATA%\runsite` on
Windows) and holds one entry per profile:

```toml
current_profile = "default"

[profiles.default]
api_url = "https://api.runsite.app"
api_key = "ak_live_..."
current_project_id = "..."
```

Select a profile with `--profile staging` or `RUNSITE_PROFILE=staging`.
`RUNSITE_API_TOKEN` overrides the stored key.

## Build

**Requirements:** Rust 1.75+ — install via [rustup.rs](https://rustup.rs)

```bash
cargo build              # dev build
cargo build --release    # optimized (~4.5 MB binary)
# binary: target/release/runsite
```

## Cross-compile

Install [cross](https://github.com/cross-rs/cross) (requires Docker):

```bash
cargo install cross --git https://github.com/cross-rs/cross
cross build --release --target x86_64-unknown-linux-musl
cross build --release --target aarch64-unknown-linux-musl
```

## Release

Tag the repository; the `Release` workflow builds all five targets, generates
`SHA256SUMS.txt` and publishes a GitHub Release:

```bash
cargo set-version 0.2.0   # or edit Cargo.toml
git tag v0.2.0 && git push origin v0.2.0
```

## License

MIT
