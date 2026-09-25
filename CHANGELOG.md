# Changelog

Notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versioning follows
[Semantic Versioning](https://semver.org/). While the version is 0.x, a breaking change
bumps the minor version.

## [Unreleased]

### Changed

- **Breaking:** `FieldChange` gains `unstated` and `ChangeSet` gains `omitted`. Struct
  literals must set them (`None` for both); in JSON they are new keys, present only when set.
- **Breaking:** Built on the current `uncad-model` API. Field paths follow its JSON form, in
  which a polyline vertex is a point and a bulge: `vertices[i].point.x` where `vertices[i].x`
  was reported before. Geometric matching orders polylines by their first vertex's point.

### Added

- `FieldChange::unstated` (`BEFORE` or `AFTER`, type `Side`) marks a field change where one
  side is the model's `null` -- a value the file did not state -- and the other is not, so a
  drawing saved again in a newer format can be told apart from an edit. The verdict is kept.
- `ChangeSet::without(Omit)` projects a change set without the field changes within
  tolerance, those one side does not state, or both; `ChangeSet::omitted` (`Omitted`) counts
  what it left out. A change set as `diff` returns it carries no `omitted`.

## [0.1.0] - 2026-09-22

Initial release. `diff(&before, &after, options)` compares two
[uncad-model](https://github.com/iyulab/uncad-model) drawing states, matched by entity
reference ID (`Matching::Reference`, the default) or, for revisions that share no references,
by entity type and shape within tolerance (`Matching::Geometry`, where only a match certain both
ways counts and the rest is `UNKNOWN` with its candidates). It returns the exact change set:
added, removed and modified entities, every differing field of a modified one with its delta
and a within/beyond verdict against an explicit tolerance, in a fixed order, byte for byte the
same every time.
