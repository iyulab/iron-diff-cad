# Changelog

Notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versioning follows
[Semantic Versioning](https://semver.org/). While the version is 0.x, a breaking change
bumps the minor version.

## [Unreleased]

### Added

- `lineage(&before, &after, threshold)` and the change set's `lineage`: whether two states are one
  drawing -- `SAME`, `DIFFERENT` or `UNKNOWN` -- judged from the header's `$FINGERPRINTGUID`, the
  reference IDs both states hold and whether any of them names entities of two types, with every
  fact the verdict was reached from (`fingerprint_equal`, `version_equal`, `shared`, `smaller`,
  `cross_type`, `threshold`). Carried by every change set, whatever the mode.
- `Matching::Auto`: reference matching when the lineage is `SAME`, geometric matching otherwise.
  The change set's `matching` is always the mode used. `DiffOptions::shared_threshold` sets the
  share of shared IDs `SAME` asks for (`DEFAULT_SHARED_THRESHOLD`, a half).

### Changed

- **Breaking:** `DiffOptions::default()` matches by `Matching::Auto`, no longer by reference: two
  unrelated drawings whose IDs coincide were paired into confident field changes. A caller that
  compares two states it knows are one drawing (before and after its own edit) asks for
  `Matching::Reference`. `Matching` has a new variant and `DiffOptions` a new field.

### Fixed

- Under reference matching, a reference held by entities of different types in the two states
  is reported as `REMOVED` plus `ADDED`, no longer as a `MODIFIED` entity whose fields include
  the type. Two unrelated drawings whose IDs happen to coincide gave confident field changes.
- Under geometric matching, a nested entity's `common` block (an INSERT's attributes) is no
  longer part of the shape, and its reference ID and source handle are never compared fields. An
  unchanged INSERT with attributes was reported as `REMOVED` plus `ADDED` between two revisions.
- A DIMENSION's `block_name` naming an anonymous block (`*D3`) is not part of the shape, and is
  not compared when both sides name one: a save renumbers
  the anonymous block a dimension is drawn with (`*D3` to `*D4`) while the dimension stays as it
  was, so the same drawing saved again reported every dimension as `MODIFIED` under reference
  matching and as `REMOVED` plus `ADDED` under geometric matching. A dimension repointed to a
  named block, or from one, is still a field change.

## [0.3.0] - 2026-10-02

### Changed

- Built on `uncad-model` 0.3.0 (a drawing's `header`; a multileader's leader roots), so it
  compares drawings of that model.

## [0.2.0] - 2026-09-29

### Changed

- Geometric matching no longer compares every entity with every other. Candidates are looked
  for among the entities of the same type whose place -- the representative point, or the
  first point of an entity without one -- lies within the length tolerance, found in a sorted
  window. The change set is the same; a drawing of 10 000 entities is matched in seconds rather
  than a quarter of an hour.

- **Breaking:** `ChangeSet`, `FieldChange`, `EntityRecord`, `Modified`, `Unknown` and
  `Omitted` are `#[non_exhaustive]`, so a field added later is not a breaking change. A change
  set is built by `diff` (or deserialized from its JSON form), not by struct literal. `Change`,
  `Verdict` and `Side` stay exhaustive: a new kind of change is one every consumer must handle.
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
