# AGENTS.md — read this first

This file is for every contributor, human or AI agent. Read it completely before
writing code, docs or tests.

## 1. Clean-room protocol (mandatory, applies to every change)

GroupKeep is one of the MoneyKeep Apps, a clean-room implementation of personal and
small-business finance software. MoneyKeep's full protocol, with its role roster and its
list of prior-art projects, is in `docs/clean-room.md` of the MoneyKeep repository
(`moneykeepproject/moneykeep`). The short version, which you must follow here too:

- **Know your role for the task.** You are either a **spec writer** (may study target
  products and prior art through use and public docs; writes only specs) or an
  **implementer** (writes code from specs and public standards only). Never both
  for the same feature area.
- **Implementers never read** source code of commercial finance products, any
  proprietary product, or any project listed under "Prior art" in MoneyKeep's
  `docs/clean-room.md`, whatever its licence (GPL, AGPL, LGPL, BSL, source-available,
  MIT or BSD).
- **Never** decompile, disassemble or reverse-engineer target binaries or their
  proprietary file formats.
- **Never** copy trademarks of commercial finance products, or their icons, artwork,
  report layouts, help text or UI wording.
- **Never** scrape or automate bank websites (browser automation is out of scope:
  institutions' terms of use prohibit it).
- If you are an AI agent and you are given, or come across, forbidden material: stop,
  do not use it, and say so in your output.

## 2. Workflow

```sh
cargo run -p groupkeep                      # the desktop app
cargo fmt --all -- --check                  # formatting
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                      # the division engine, the app and the installer script checks
cargo xtask release [--dry-run]             # dist/GroupKeep-<version>/, THIRD-PARTY-LICENSES.txt included
cargo xtask installer                       # the Windows setup (on Windows, needs NSIS);
                                            # on a Mac: packaging/macos/package.sh, the .dmg
packaging/macos/package.sh --dry-run        # the macOS plan, on any system
cargo xtask workflows                       # the Mac runner rules for .github/workflows
```

Run fmt, clippy and the tests before every commit. They must pass.

## 3. Shipping

- `THIRD-PARTY-LICENSES.txt` (every crate compiled into the program, with its version,
  licence and the licence texts it ships) is written by `cargo xtask release` from
  `cargo metadata` and the crates' own licence files. The installer installs it beside
  the program.
- Slint is used under the Slint Royalty-free License 2.0; the README keeps Slint's
  attribution badge.
- GroupKeep keeps no data of its own (groups are `.groupkeep` files the user saves), so
  the uninstaller removes only the files Setup installed.

## 4. Releases and the Mac

- A release is a pushed tag `v<version>` (the workspace version in Cargo.toml) or a hand run of
  `.github/workflows/release.yml`; it makes the draft GitHub Release with
  `groupkeep-<version>-windows-x64-setup.exe`, `groupkeep-<version>-macos-universal.dmg` and
  `SHA256SUMS.txt` (MoneyKeep's `docs/specs/macos-release.md`, batch MR3).
- The macOS job runs on the project's own Mac, a self-hosted runner, after the owner approves it
  in the `mac-release` environment. Only `release.yml` may name `self-hosted`, and only with a
  tag push and `workflow_dispatch` as triggers: `cargo xtask workflows` (also a test) fails
  otherwise. The owner's runbook is `docs/deploy/mac-runner.md` in the MoneyKeep repository.
- `packaging/macos/` makes `GroupKeep.app` (`com.moneykeep.groupkeep`, macOS 11.0 or later,
  universal) and the disk image. It signs ad hoc until the project has a Developer ID; then it
  signs with the Mac's `mk-signing` keychain and notarizes with its `mk-notary` profile. Never
  put a password or Apple ID on a command line. Shell scripts are stored executable in git (a
  test checks).
