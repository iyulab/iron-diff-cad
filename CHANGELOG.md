# Changelog

Notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versioning follows
[Semantic Versioning](https://semver.org/). While the version is 0.x, a breaking change
bumps the minor version.

## [Unreleased]

## [0.9.0] - 2026-10-07

### Changed

- Built on `uncad-model` 0.9.0.

## [0.8.0] - 2026-10-07

### Changed

- Built on `uncad-model` 0.8.0.

## [0.7.0] - 2026-10-07

### Changed

- Built on `uncad-model` 0.7.0.

## [0.6.0] - 2026-10-07

### Changed

- Built on `uncad-model` 0.6.0.

## [0.5.0] - 2026-10-05

### Changed

- Built on `uncad-model` 0.5.0: a table's grid is one of its fields, so a change to a table's
  cells is a field change of the table.

## [0.4.0] - 2026-10-04

### Added

- `lineage(&before, &after, threshold)` and the change set's `lineage`: whether two states are one
  drawing -- `SAME`, `DIFFERENT` or `UNKNOWN` -- judged from the header's `$FINGERPRINTGUID`, the
  reference IDs both states hold and whether any of them names entities of two types, with every
  fact the verdict was reached from (`fingerprint_equal`, `version_equal`, `shared`, `smaller`,
  `cross_type`, `threshold`). Carried by every change set, whatever the mode.
- `Matching::Auto`: reference matching when the lineage is `SAME`, geometric matching otherwise.
  The change set's `matching` is always the mode used. `DiffOptions::shared_threshold` sets the
  share of shared IDs `SAME` asks for (`DEFAULT_SHARED_THRESHOLD`, a half).
- Geometric matching pairs entities whose shape changed. After the entities whose shapes agree are
  paired, an entity left without a counterpart is scored against those of its type left in the
  other state: the share of their informative leaf values that agree within tolerance (a leaf
  every entity of the type holds with one value tells none apart and is not counted). A pair is
  taken when its similarity reaches `min_similarity`, each is the other's single highest score,
  and the next highest score of either is at least `min_margin` lower; it is `MODIFIED`, with
  `Modified::matched_by` (`MatchedBy`: the similarity and the runner-up). Short of that, with
  candidates at the threshold, the entity is `UNKNOWN` with `Unknown::similarities`; a tie is
  never broken.
- `DiffOptions::pairing` (`Pairing`: `min_similarity` 0.5, `min_margin` 0.1, `max_pairs` ten
  million) and the change set's `pairing`, the values used under geometric matching. A
  `min_similarity` above 1 pairs by equal shape only, as before.
- `ChangeSet::unscored` (`Unscored`): an entity type whose left-over entities would take more
  scored pairs than `max_pairs` leaves, with that count. Its entities stay `REMOVED` and `ADDED`.

### Changed

- An INSERT or ACAD_TABLE whose `block_name` names an anonymous block on both sides (a dynamic
  block's current state, `*U24` → `*U96`; a table's `*T`) is compared by what the two blocks hold:
  when they hold the same entities in the same order within tolerance, and the same base point,
  the renumbering a save does is no longer a `block_name` change -- nested anonymous blocks
  included, 20 deep. Blocks that hold something else keep the change with both names. An anonymous
  `block_name` is no longer part of an INSERT's or a table's shape, so geometric matching pairs
  them and then applies the same rule.
- An IMAGE's `definition` -- the image definition's handle, identity rather than content -- is no
  longer part of its shape, and a change of it is left out when the two image definitions state the
  same file, size and pixel size: an image whose definition was only renumbered read as a
  `definition` change, and under geometric matching could not be paired by shape.
- **Breaking:** `DiffOptions::default()` matches by `Matching::Auto`, no longer by reference: two
  unrelated drawings whose IDs coincide were paired into confident field changes. A caller that
  compares two states it knows are one drawing (before and after its own edit) asks for
  `Matching::Reference`. `Matching` has a new variant, and `DiffOptions` two new fields,
  `shared_threshold` and `pairing`: a struct literal adds them, or `..DiffOptions::default()`.
- Under geometric matching, an entity whose shape changed (a resized hole, a moved line) is
  `MODIFIED` when the pairing above singles out its counterpart, no longer `REMOVED` and `ADDED`.
- Built on `uncad-model` 0.4.0 (a multileader's line type and content; the header's drawing
  identifiers).

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
