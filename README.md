# ga4-omarchy

`ga4` is a fast, keyboard-driven Google Analytics 4 dashboard for
[Omarchy](https://omarchy.org/). It uses your terminal's ANSI palette, so it
follows the active Omarchy theme without maintaining a separate theme file.

The focused MVP includes:

- Overview metrics, previous-period deltas, daily users, and acquisition channels
- Realtime users, pages, events, and a 30-minute trend
- Browser OAuth with refresh tokens stored only in the desktop keyring
- Searchable GA4 property selection, 7/28/90-day and custom date ranges
- Fifteen-minute historical cache with stale-data fallback
- CSV and JSON export, machine-readable diagnostics, and an Omarchy launcher

## Requirements

- Omarchy on x86_64 (the supported v0.1 platform)
- A running Secret Service provider such as GNOME Keyring or KWallet
- A Google account with access to at least one GA4 property
- Your own Google Cloud Desktop OAuth client

## Google Cloud setup

1. Create or select a project in the [Google Cloud console](https://console.cloud.google.com/).
2. Enable **Google Analytics Data API** and **Google Analytics Admin API**.
3. Configure the Google Auth consent screen. During development, add your Google
   account as a test user.
4. Create an OAuth client with application type **Desktop app**.
5. Download the client JSON and authorize:

   ```bash
   ga4 auth login --client-secret ~/Downloads/client_secret.json
   ```

The app copies the client definition to
`~/.config/ga4-omarchy/oauth-client.json` with mode `0600`. OAuth tokens are
stored only in the system keyring under service `ga4-omarchy`.

## Usage

Launch from the Omarchy application launcher or run:

```bash
ga4
```

Common commands:

```bash
ga4 auth status
ga4 properties
ga4 properties --json
ga4 export --property 123456789 --start 2026-08-01 --end 2026-08-31 --format csv
ga4 doctor
ga4 cache clear
ga4 auth logout
```

The TUI uses these keys:

| Key | Action |
| --- | --- |
| `1`, `2` | Overview or Realtime |
| `←`, `→` | Move the historical date range |
| `[`, `]` | Select a shorter or longer preset |
| `Tab` | Move pane focus |
| `j`, `k` | Move row selection |
| `Enter`, `Esc` | Drill in/confirm or go back |
| `/` | Filter the active table |
| `c` | Toggle previous-period comparison |
| `p` | Select a property |
| `d` | Enter a custom date range |
| `r` | Refresh |
| `e` | Export the Overview to CSV |
| `?`, `q` | Help or quit |

Set `NO_COLOR=1` for a monochrome interface.

## Local data

Preferences live in `~/.config/ga4-omarchy/config.toml`. Aggregated historical
reports are cached under `~/.cache/ga4-omarchy/reports/`; Realtime responses are
never written to disk. `ga4 cache clear` removes only the report cache, and
`ga4 auth logout` removes only the token from the keyring.

## Development

The repository pins Rust 1.88 and can bootstrap it with Mise:

```bash
mise install
mise exec -- cargo test
mise exec -- cargo clippy --all-targets --all-features -- -D warnings
```

Tests use fixtures and local mock servers. No Google credentials or report data
belong in the repository or CI.

## Release process

1. Update the version in `Cargo.toml` and record user-visible changes.
2. Run formatting, Clippy, tests, and `scripts/package-release.sh`.
3. Tag the commit as `vX.Y.Z` and push the tag; GitHub Actions publishes the tarball and checksum.
4. Replace the AUR `sha256sums` value with the published source archive checksum.
5. Regenerate `.SRCINFO` with `makepkg --printsrcinfo > .SRCINFO`, then push the package to AUR.

The project intentionally does not edit `~/.config/omarchy/` or anything under
`/usr/share/omarchy/`.

## License

GPL-3.0-only. See [LICENSE](LICENSE).
