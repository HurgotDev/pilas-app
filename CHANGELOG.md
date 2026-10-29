# Changelog

All notable changes to Pilas are recorded in this file. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
follows [Semantic Versioning](https://semver.org/).

Sections are generated with `npm run release:prepare` from the fragments in
`.changes/` (see `docs/RELEASING.md`).

## [0.3.0] - 2026-10-28

### Added

- Leave requests to the terminal, without opening the island or playing a sound, when the Ghostty tab of the session that asks is already focused. Works with normal and background sessions.
- Add a dizzy sound when the pilita gets dizzy after three quick clicks.
- Add the launch at login toggle to the onboarding.
- Add seasonal costumes for the pilita, starting with a Halloween one with its own sounds.

### Fixes and improvements

- Hide the pending counter when nothing is pending instead of showing 0.
- Open the island by clicking the pilita; clicking it again no longer closes it.
- Scroll the options of a question card when they do not fit instead of cutting them off.
- Release PRs go straight to main with an automatic back-merge into develop; commits, changelog, fragments and README are now in English.

## [0.2.0] - 2026-10-28

### Added

- Add signed automatic updates, installation with Homebrew, and the hook
  refresh on startup.

### Fixes and improvements

- The CI also builds, lints and tests the app (src-tauri).
- Update the README (what it does, privacy and commands) and add the rule to
  keep it up to date.
- Add the branch and release flow with a changelog fragment per change.

## [0.1.0] - 2026-10-28

### Added

- Island over the notch as a floating panel that does not steal focus, with a
  smooth transition from pill to card.
- List of Claude Code sessions with status, project, last action and cleanup
  of orphaned sessions.
- Permission cards with Allow, Always and Deny, in a FIFO queue and with
  keyboard shortcuts.
- Answers to AskUserQuestion questions (one to four, single or multiple
  choice) without leaving the island.
- The animated pilita with states, reactions and configurable sounds.
- Interface in English and Spanish, depending on the system language.
- Jump to the exact Ghostty tab and activate other terminals.
- Hook installer with backup and diff preview, onboarding and settings;
  universal `.dmg` for Apple Silicon and Intel.
