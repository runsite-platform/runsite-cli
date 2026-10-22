# Contributing

Thanks for helping improve the Runsite CLI. Bug reports, ideas and pull requests are all welcome.

## Reporting bugs and ideas

Use the [issue templates](https://github.com/runsite-platform/runsite-cli/issues/new/choose). For security problems, follow [SECURITY.md](SECURITY.md) instead.

## Development setup

You need Rust 1.75+ ([rustup.rs](https://rustup.rs)).

```bash
git clone https://github.com/runsite-platform/runsite-cli
cd runsite-cli
cargo run -- --help
```

Point the CLI at a different API with `runsite context set-url <url>` or a separate profile (`--profile dev`).

## Before opening a pull request

CI runs these on Linux, macOS and Windows. Run them locally first:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

TUI screens are covered by snapshot tests. If you change a screen on purpose, review and accept the new snapshots with [`cargo insta review`](https://insta.rs/docs/cli/).

## Pull request guidelines

- Branch from `develop` and target `develop`. `main` only receives releases.
- Keep each PR focused on one change.
- Use [Conventional Commits](https://www.conventionalcommits.org/): `feat(tui): ...`, `fix(cli): ...`, `docs: ...`.
- Add or update tests for behaviour changes.
- Update the README when you add or change a command.

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
