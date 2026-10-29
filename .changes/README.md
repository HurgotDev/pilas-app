# Changelog fragments

Every change merged into `develop` adds one file here, named `<slug>.md` (for
example `hook-installer.md`). When a release is prepared,
`npm run release:prepare` gathers them into `CHANGELOG.md`, computes the
version and deletes them.

Format:

```
---
bump: minor   # none | patch | minor | major
---
Add the hook installer with backup and diff.
```

- `bump`: `patch` for fixes, `minor` for new features, `major` for breaking
  changes. The highest bump across all fragments wins. `none` means no new
  version (docs, README, CI): it stays out of the changelog, and if every
  fragment is `none` the release keeps the current version.
- Body: one sentence in English, in the imperative. It becomes a bullet in the
  changelog.

The tool ignores this README. More detail in `docs/RELEASING.md`.
