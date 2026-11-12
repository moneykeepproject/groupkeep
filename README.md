# GroupKeep

Divide a group's expenses. Part of the MoneyKeep family (Rust + Slint).

Version 0.1.0: [release notes](docs/release-notes/0.1.0.md) · [known limits](docs/known-limits.md).

## Build & run

```sh
git clone https://github.com/moneykeepproject/groupkeep
cd groupkeep
cargo run --release -p groupkeep              # the app
cargo run --release -p groupkeep -- "Trip.groupkeep"   # open a file
cargo test -p groupkeep-core                  # engine tests
cargo run -p groupkeep --example flow         # scripted UI test (headless)
cargo run -p groupkeep --example screens -- out 860    # headless screenshots of every screen
cargo run -p groupkeep-core --example sample -- out    # sample .groupkeep + PDF
```

## Layout

| Path | What |
|---|---|
| `core/` | `groupkeep-core`: data model, division engine, settle-up, PDF. No UI code, so it can compile to WASM for the future PWA. |
| `core/src/model.rs` | Group, people, expenses, overrides, donations; `.groupkeep` JSON format |
| `core/src/calc.rs` | Division engine + settle-up (pure functions) |
| `core/src/money.rs` | Currencies, integer minor units, parse/format |
| `core/src/pdf.rs` | US Letter PDF report (built-in Helvetica, no font files) |
| `app/ui/*.slint` | Mobile-first UI: one column, 48px tap targets, bottom tabs |
| `app/src/controller.rs` | Connects the UI to the core |

## Group types

| Type | Default method | Extras |
|---|---|---|
| Simple | Equal | |
| Meal | By item | Items, tax, tip (% or amount) |
| Day Trip | Equal | Several dated expenses |
| Vacation | By attendance | Trip dates; days each person was there |
| Community Event | Shares (household size) | Rows are households; donations & sponsors |
| All Others | Equal | Every method; donations |

## Division methods

Equal • Shares (weights, e.g. child = 0.5) • Percent • Exact amounts • By item (tax and tip follow each person's subtotal) • By attendance (each covered day is split among those present).

## Overrides

Every override needs a description, a person and a scope (one expense or the whole group):

* **Pays exactly** an amount
* **Add / subtract** an amount
* **Change by percent** (-50 = half off)
* **Pays nothing** (excluded)

The difference is re-split among the others in proportion to their shares. Overrides appear on their own tab (with a count badge), on each expense card, highlighted on the person's row in the expense editor, and in their own section of the PDF.

## Rounding

All money is integer minor units (cents; yen has none). Leftover pennies go to the expense's payer. Group-wide adjustments and donation credits go to the top payer. Every rounding is listed in the app and the PDF.

## Currencies

USD, EUR, JPY, GBP, CNY, AUD, CAD. Currency is set per group. Switching currency keeps face values; it does not convert at an exchange rate.

## Files

`.groupkeep` is pretty-printed JSON with a `format_version` field. Unknown or missing fields fall back to defaults, so older files keep opening.

## Windows installer (NSIS)

Same setup as CalcKeep and MoneyKeep: the script is `installer/groupkeep.nsi`, built by an xtask.

```powershell
winget install NSIS.NSIS        # once (or https://nsis.sourceforge.io/Download); then open a new terminal
cargo xtask installer           # -> dist\groupkeep-<version>-windows-x64-setup.exe and dist\SHA256SUMS.txt
```

- `cargo xtask release` builds the release program and gathers `dist\GroupKeep-<version>\` (groupkeep.exe, LICENSE-MIT, LICENSE-APACHE, NOTICE, README.md). `--dry-run` lists the steps only.
- `cargo xtask installer` runs `release`, then `makensis`, then writes `SHA256SUMS.txt` (`<sha256>  <name>`) beside the setup. `--skip-release` reuses an existing release folder.
- The version comes from `Cargo.toml` (`[workspace.package] version`). Bump it there for each release; Windows' four-number version is derived from it (`0.1.0-alpha.1` → `0.1.0.1`).
- **Release files:** publish `groupkeep-<version>-windows-x64-setup.exe` and `SHA256SUMS.txt` on the GitHub release. These are the names MoneyKeep Launcher looks for: it checks the setup against `SHA256SUMS.txt` and installs, updates and removes GroupKeep with it.
- **The setup:** installs for all users in `C:\Program Files\GroupKeep`, so it asks for administrator permission. It shows the licence (NOTICE, MIT and Apache-2.0), has a Start menu shortcut (on) and a Desktop shortcut (off), registers in Installed apps (publisher MoneyKeep Project), updates in place, and asks you to close GroupKeep if it's open. GroupKeep keeps no data of its own: your `.groupkeep` files stay wherever you saved them, and the uninstaller never touches them. The icon is `assets/icon.ico` (made from `assets/icon.svg`, with the simpler `assets/icon-small.svg` for 16–32 px); the build embeds it in `groupkeep.exe` (see `app/build.rs`) and the setup uses it for its own window, the shortcuts and Installed apps.
- **Silent mode** (what MoneyKeep Launcher uses): `groupkeep-<version>-windows-x64-setup.exe /S` installs or updates with the default options and no questions (`/D=<folder>` picks the folder of a new install). If GroupKeep is open it is asked to close (with unsaved changes it stays open), and after 30 s the setup stops with exit code 2; 0 means done, 3 any other failure. Windows still asks for administrator permission. The silent uninstall is `"C:\Program Files\GroupKeep\uninstall.exe" /S` (`QuietUninstallString`).
- **Signing:** set `GK_SIGN_CERT` (path to a `.pfx`) and `GK_SIGN_PASSWORD` to sign groupkeep.exe and the setup with `signtool`. `GK_SIGN_TIMESTAMP` overrides the timestamp server. Without them the setup is unsigned and SmartScreen will warn.
- `cargo test -p xtask` checks the script's silent mode and compiles it with a placeholder program when NSIS is installed.

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your option.

The user interface is built with [Slint](https://slint.dev), used under the Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0; this badge is its attribution (licence §2(b)):

[![Made with Slint](https://raw.githubusercontent.com/slint-ui/slint/master/logo/MadeWithSlint-logo-whitebg.png)](https://slint.dev)

Every third-party crate compiled into GroupKeep is listed with its licence in `THIRD-PARTY-LICENSES.txt`, which `cargo xtask release` writes and the installer installs beside the program.
