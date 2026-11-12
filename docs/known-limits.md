# Known limits

What GroupKeep 0.1.0 doesn't do yet. This list is updated with each release. For what
it does do, see the [release notes](release-notes/0.1.0.md).

## Splitting

- **One currency per group.** Switching a group's currency keeps the amounts as they
  are; it doesn't convert them at an exchange rate.
- **Seven currencies:** US dollar, euro, Japanese yen, British pound, Chinese yuan,
  Australian dollar and Canadian dollar.
- **Leftover cents** always go to the person who paid that expense, and leftovers from
  group-wide adjustments and donation credits to the person who paid the most. Every
  rounding is listed in the app and the PDF.
- **The PDF report is US Letter size** only.

## Your files

- **GroupKeep keeps no data of its own.** A group is only kept if you save it as a
  `.groupkeep` file, so save your groups and keep copies of the ones that matter.

## Installing

- **The Windows setup is not signed yet.** Windows SmartScreen may warn: choose
  **More info**, then **Run anyway**.
- **The Mac build is not signed with an Apple Developer ID yet** (it is signed "ad
  hoc"). macOS warns the first time you open it: right-click GroupKeep, then choose
  **Open** (or System Settings > Privacy & Security > **Open Anyway**).
- **MoneyKeep Launcher can't install GroupKeep on a Mac yet.** It installs only Mac
  builds signed with the project's Developer ID. Until then, install from the
  release's `.dmg`; the launcher opens the copy you installed and links to new
  releases.
- **Release files are for Windows (64-bit) and Mac only.** There is no phone or web
  version yet.
