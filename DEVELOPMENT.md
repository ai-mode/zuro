# Development Guide

## Release Process

### Versioning

This project follows [Semantic Versioning](https://semver.org):

- `patch` — bug fixes, no API changes
- `minor` — new functionality, backwards compatible
- `major` — breaking changes

### Routine Release

1. Features and fixes are merged into `master` through pull requests
2. When ready to release, create a branch: `release/vX.Y.Z`
3. In a single commit:
   - Bump version in `Cargo.toml`
   - In `CHANGELOG.md`: rename `[unreleased]` to `[vX.Y.Z]` with today's date,
     add a fresh empty `[unreleased]` section at the top, update the compare link
4. Open a PR `release/vX.Y.Z → master`, get it reviewed and merged
5. Tag the merge commit: `git tag vX.Y.Z`
6. Push the tag: `git push --tags`
7. Publish: `cargo publish`

### Hotfix

1. Branch off the tag: `git checkout -b hotfix/vX.Y.Z vX.Y.(Z-1)`
2. Apply the fix, bump the patch version, update `CHANGELOG.md`
3. Open a PR into `master`
4. Tag and publish as above

---

## Changelog Format

`CHANGELOG.md` follows this structure. Every release gets its own section;
unreleased work accumulates at the top.

```markdown
## [unreleased](https://github.com/ai-mode/zuro/compare/vX.Y.Z...HEAD) - XXXX-XX-XX

### Changes
_Changes of existing functionality_

### New Features

### Bugfixes
_For any bug fixes_

### Security
_Security vulnerabilities improvements_

### Build/Testing/Packaging
_All changes related to github actions, packages_

### Other
_Other cases_

---

## [vX.Y.Z](https://github.com/ai-mode/zuro/releases/tag/vX.Y.Z) - XXXX-XX-XX
```

Rules:
- Only include sections that have entries — omit empty ones
- Use plain English, imperative mood: "Add X", "Fix Y", not "Added X", "Fixed Y"
- Reference issues or PRs where relevant: `Fix crash on empty config (#12)`
