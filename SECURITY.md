# Security Policy

## Supported versions

Only the latest release of the Runsite CLI gets security fixes. Upgrade by running the install script again.

## Reporting a vulnerability

Please **do not** open a public issue for security problems.

Report it privately through GitHub: open the **Security** tab of this repository, then choose **Report a vulnerability**. Include:

- the CLI version (`runsite --version`) and your OS
- the steps to reproduce
- what an attacker could do with it

We aim to reply within 3 working days and to ship a fix or a mitigation as soon as we can confirm the issue.

## Scope

In scope: this CLI, the install scripts in `install/`, and the release binaries.

Problems with the Runsite platform itself (dashboard, API, hosting) are also welcome through the same channel.

## Handling API keys

The CLI stores your API key in `~/.config/runsite/config.toml` (`%APPDATA%\runsite` on Windows). Treat this file like a password. In CI, use the `RUNSITE_API_TOKEN` secret instead of a stored key, and revoke any key you think has leaked from the dashboard.
