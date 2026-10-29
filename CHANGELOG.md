# Changelog

All notable changes to Pilas are recorded in this file. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
follows [Semantic Versioning](https://semver.org/).

Sections are generated with `npm run release:prepare` from the fragments in
`.changes/` (see `docs/RELEASING.md`).

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
