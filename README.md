<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo-runsite-dark.svg">
    <img src="assets/logo-runsite.svg" alt="Runsite" width="220" />
  </picture>
</p>

<h1 align="center">Runsite CLI</h1>

<p align="center">
  Deploy and manage your Runsite services from the terminal — web services,
  deployments, logs and environment variables, scriptable from any CI.
</p>

<p align="center">
  <a href="https://runsite.app">runsite.app</a> ·
  <a href="https://docs.runsite.app">Documentation</a> ·
  <a href="https://docs.runsite.app/cli/overview/">CLI Docs</a> ·
  <a href="https://docs.runsite.app/api-reference/public-api/">API Reference</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/built%20with-Rust-DEA584?logo=rust&logoColor=black" alt="Rust" />
  <img src="https://img.shields.io/badge/platform-Linux%20%C2%B7%20macOS%20%C2%B7%20Windows-4169E1" alt="Platforms" />
  <img src="https://img.shields.io/badge/license-MIT-green" alt="MIT" />
</p>

---

## What is the Runsite CLI?

`runsite` is a single static binary that talks to the [Runsite](https://runsite.app)
public API. It lets you trigger deployments, tail logs, manage environment variables
and switch between projects without leaving your shell — interactively on your
machine, or non-interactively in a CI pipeline with an API token.

## Install

**Linux / macOS:**

```bash
curl -fsSL https://raw.githubusercontent.com/runsite-platform/runsite-cli/main/install/install.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/runsite-platform/runsite-cli/main/install/install.ps1 | iex
```

The scripts detect your OS and architecture, download the matching binary from
GitHub Releases, verify its SHA-256 checksum, and install it
(`~/.local/bin/runsite` on Unix, `%LOCALAPPDATA%\runsite\bin\runsite.exe` on Windows).

| Variable | Purpose |
|---|---|
| `RUNSITE_VERSION` | Pin a release, e.g. `v0.1.0` (default: latest) |
| `RUNSITE_INSTALL_DIR` | Override the install directory |
| `RUNSITE_REPO` | Override the source repository |

Prebuilt targets: `x86_64`/`aarch64` Linux (musl, static), `x86_64`/`aarch64` macOS,
`x86_64` Windows.

## Authenticate

```bash
runsite login                          # email + password, mints an API key
runsite login --token ak_live_...      # use a key created in the dashboard
```

`--token` is the path for accounts that sign in with Google or GitHub and have no
password. In CI, skip `login` entirely and set `RUNSITE_API_TOKEN=ak_live_...`.

## Commands

| Group | Commands |
|---|---|
| **Auth** | `login [--token]` · `logout` · `whoami` |
| **Services** | `service list [--all]` · `service create` · `service status` · `service start\|stop\|restart` |
| **Deploys** | `deploy [service]` — trigger a deployment |
| **Deployments** | `deployments list [service] [--limit 10]` · `deployments rollback <id> [service]` |
| **Logs** | `logs [service] [--tail 100]` — recent container logs |
| **Env vars** | `env list` · `env set KEY=VALUE ...` · `env delete KEY` |
| **Projects** | `project list` · `project use [name\|id]` · `project unset` |
| **Context** | `context show` · `context set-url https://api.runsite.app` |
| **Shell** | `completions bash\|zsh\|fish` |

```bash
runsite service list
runsite service create api --repo https://github.com/me/api --port 3000 --env LOG_LEVEL=info
runsite deploy api --output json
runsite deployments list api
runsite deployments rollback 3f2a1b4c api
runsite env set api DATABASE_URL=postgres://... LOG_LEVEL=debug
runsite logs api --tail 200
```

Every data command accepts `--output json` for scripting. The service argument is
optional: with a single service (or a single service in the selected project) the
CLI resolves it automatically.

### Not in this release

`runsite shell`, `runsite run` and `runsite deploy --watch` need a live WebSocket,
which the public API does not expose to API keys yet. The commands exist and point
you to the dashboard instead.

## Configuration

Config lives at `~/.config/runsite/config.toml` (`%APPDATA%\runsite` on Windows) and
holds one entry per profile:

```toml
current_profile = "default"

[profiles.default]
api_url = "https://api.runsite.app"
api_key = "ak_live_..."
current_project_id = "..."
```

Select a profile with `--profile staging` or `RUNSITE_PROFILE=staging`.
`RUNSITE_API_TOKEN` overrides the stored key.

## Repository layout

```
.
├── src/
│   ├── main.rs         # entry point
│   ├── cli.rs          # clap command definitions
│   ├── commands/       # one module per command group
│   ├── api/            # public API client + response types
│   ├── config/         # profile config file handling
│   ├── output/         # table / JSON formatting
│   └── ws/             # WebSocket transports (logs, shell)
├── install/            # install.sh and install.ps1
└── .github/workflows/  # CI and multi-target release builds
```

## Build

**Requirements:** Rust 1.75+ — install via [rustup.rs](https://rustup.rs)

```bash
cargo build              # dev build
cargo build --release    # optimized (~4.5 MB binary)
# binary: target/release/runsite
```

### Cross-compile

Install [cross](https://github.com/cross-rs/cross) (requires Docker):

```bash
cargo install cross --git https://github.com/cross-rs/cross
cross build --release --target x86_64-unknown-linux-musl
cross build --release --target aarch64-unknown-linux-musl
```

### Release

Tag the repository; the `Release` workflow builds all five targets, generates
`SHA256SUMS.txt` and publishes a GitHub Release:

```bash
cargo set-version 0.2.0   # or edit Cargo.toml
git tag v0.2.0 && git push origin v0.2.0
```

## Documentation

- **CLI guide:** [docs.runsite.app/cli/overview](https://docs.runsite.app/cli/overview/)
- **Public API:** [docs.runsite.app/api-reference/public-api](https://docs.runsite.app/api-reference/public-api/)
- **All docs:** [docs.runsite.app](https://docs.runsite.app)
- **Dashboard:** [runsite.app](https://runsite.app)

## License

MIT — see [LICENSE](LICENSE).
