# The change set

> This document describes the contract currently in force for what this library returns. A sentence here that is wrong is a bug. The rules it follows are in [principles.md](principles.md).

A change set is the exact difference between two drawing states, expressed in the [uncad-model](https://github.com/iyulab/uncad-model) entity model's terms: numbers, references, names. It is what a caller keeps as the evidence of an edit, and what a renderer draws as an overlay.

## 1. One change per entity

Every entry in a change set is a verdict about one entity. The kinds are a closed set:

| Kind | Meaning |
|---|---|
| `ADDED` | The entity is in the second state and has no counterpart in the first |
| `REMOVED` | The entity is in the first state and has no counterpart in the second |
| `MODIFIED` | The entity is in both, and at least one of its fields differs |
| `UNKNOWN` | The library could not decide, with certainty, which entity of the second state corresponds to this entity of the first. It lists the candidates (never empty: an entity with no candidate is `REMOVED`) and the reason. This is a normal result, not an error |

A confident wrong match would hide a real change, so an uncertain match is never resolved by picking the likeliest pair. Where a human would see "moved", a comparison without shared references reports `REMOVED` plus `ADDED`; that is the intended cost.

### Fields of a `MODIFIED` entry

A `MODIFIED` entry carries the list of fields that were compared, one item per field path (`radius`, `center.x`, `layer`, `text_override`, ...):

- **Numeric fields**: `before`, `after`, `delta` (the absolute difference), the `tolerance` that was applied, and a `verdict` -- `WITHIN` or `BEYOND` (section 2). A field that moved *within* tolerance stays in the list: "it moved, but not beyond tolerance" is information, and hiding it would make it indistinguishable from "it did not move". A consumer that wants only the significant changes filters on the verdict.
- **Non-numeric fields** (names, text, flags): `before` and `after`, and they appear only when they differ. There is no tolerance and no delta.
- **A value one side does not state**: the model writes `null` where a file did not state a value, and documents per field what that means (a format older than the field, an optional group left out). When exactly one side holds that `null`, the field change also carries `unstated` -- `BEFORE` or `AFTER`, the side that did not state it. The verdict stays `BEYOND`: the two states do differ, and treating "not stated" as equal to some value would be inventing that value. The mark is what tells a drawing saved again in a newer format -- which states what the older format had no place for -- apart from an edit. A side that has no element there at all (an array that grew or shrank) is reported with `null` too but carries no mark: that is a change of shape.
- **Reference fields** (a layer, a block, a style -- three-state values in the model: resolved, absent, unresolved-with-handle): a change of the resolved value *or* of the state is a field change, reported with the two three-state values verbatim. A reference that stopped resolving is never swallowed as "no value".

Two fields are identity, not content, and are never compared: the reference ID (`common.id`) and the handle it was issued from (`common.source_handle`). Under reference matching they are the key; under geometric matching they differ for every pair by definition.

Every entry also carries the **provenance** of both entities involved and a **confidence**, which is the lower of the two (a change between two low-confidence values is a low-confidence change, and no change is ever reported with a higher confidence than the entities it involves). Under geometric matching a `MODIFIED` entry also carries `counterpart`, the matched entity's reference ID in the second state; under reference matching the two IDs are the same and the field is absent.

## 2. Tolerance

Every numeric comparison uses a tolerance the caller stated or a default the result states -- there is no hidden epsilon (the defaults are `length = 1e-6` and `angle = 1e-9`). Lengths and angles have separate tolerances (`length` and `angle`), in the drawing's own units, since the model carries no unit. A field is an angle when the model documents it as one, which its name says: `rotation`, `start_angle`, `end_angle`, `angle`. Every other numeric field, including dimensionless ones such as scale factors and ratios, is compared with the length tolerance.

The inequality is fixed:

- `|delta| < tolerance` is `WITHIN`;
- anything else, including `|delta| == tolerance`, is `BEYOND`. Something that moved by exactly the tolerance has moved.

The comparison is made on the `f64` difference as computed, with no rounding: a caller who wants the boundary to fall on a particular value states a tolerance that value can reach exactly in binary floating point.

The tolerances that were applied are written into the change set's header, so the same change set cannot be read against two different tolerances.

## 3. Matching modes

| Mode | When | Matching key | In the result |
|---|---|---|---|
| `REFERENCE` | The two states share entity reference IDs (before and after an operation on the same model) | The reference ID, exactly as the model issued it -- this library defines no reference scheme of its own | A reference present in only one state is `ADDED` or `REMOVED` |
| `GEOMETRY` | The two states share no references (two revisions of a drawing) | Entity type plus shape, equal within tolerance | An uncertain correspondence is `UNKNOWN`; a moved entity is `REMOVED` plus `ADDED` |

The header names the mode that produced the change set, so the same output cannot be read in two meanings.

### Geometric matching

An entity's **shape** is every field of its model form except the `common` block (identity, provenance, confidence, layer, colour) and the type tag: the geometry and values that say what the entity *is*. Two entities have the same shape when they are of the same type and a field-by-field comparison (section 1, with the tolerance of section 2) finds no numeric field `BEYOND` and no non-numeric field different.

An entity `x` of the first state and `y` of the second are **counterparts** only when the match is certain both ways: `y` is the only entity of the second state with `x`'s shape, and `x` is the only entity of the first state with `y`'s. A counterpart pair is then compared on every field (so a layer or colour change, or a move within tolerance, is a `MODIFIED` entry with `counterpart` set). An entity with no candidate is `REMOVED` (first state) or `ADDED` (second state). Anything in between -- two candidates, or one candidate that is also another entity's only candidate -- is `UNKNOWN` for the entity of the first state, with every candidate listed; a likeliest pair is never chosen. Two coincident entities of which one was deleted are therefore two `UNKNOWN` entries, not a `REMOVED` and a match: which one went is not knowable from the geometry.

## 4. Order

The same two states produce the same change set in the same order, byte for byte. The ordering keys are part of the contract:

- In `REFERENCE` mode, entries are ordered by reference ID, ascending.
- In `GEOMETRY` mode, entries are ordered by entity type, then by the entity's representative point compared lexicographically on (x, y, z), then by its second point where the type has one. Entries from both states are ordered together (a `REMOVED` entity and the `ADDED` one that replaced it are neighbours when their points are). The representative point is the first of these fields the entity has: `start_point`, `center`, `position`, `insertion_point`, `point`, `corner1`, `base_point`, else the first element of `vertices`, `fit_points`, `control_points`, `boundary` or `lines`; the second point is the first of `end_point`, `major_axis_endpoint`, `vector`, `corner2`, `target`, else the second element of those arrays. A type with neither (a dimension, whose model form is its block reference) sorts as the origin, and entries with equal keys keep the order they were found in: first state by reference ID, then the second state's additions.
- Within a `MODIFIED` entry, fields are ordered by field path, ascending (plain string order, so `vertices[10]` sorts before `vertices[2]`).

No hash-based collection takes part in producing the output.

## 5. JSON

The serialized form follows the model's own convention: adjacently tagged, upper-case tags.

```json
{
  "matching": "REFERENCE",
  "tolerance": { "length": 0.001, "angle": 0.0001 },
  "changes": [
    { "type": "MODIFIED", "data": {
        "id": 289, "entity_type": "CIRCLE",
        "provenance": ["VECTOR", "DERIVED"], "confidence": "HIGH",
        "fields": [
          { "path": "radius", "before": 5.0, "after": 6.0, "delta": 1.0,
            "tolerance": 0.001, "verdict": "BEYOND" }
        ] } },
    { "type": "UNKNOWN", "data": {
        "id": 301, "candidates": [412, 413],
        "reason": "two identical circles, no shared reference" } }
  ]
}
```

Reference IDs are the model's `EntityId` values (integers); provenance and confidence are the model's `Origin` and `Confidence` (`VECTOR` / `RASTER` / `DERIVED`, `UNKNOWN` / `LOW` / `HIGH`). Values and names above are illustrative.

## 6. What the change set is not

- **Not an inspection of a state.** Whether a dimension's text still agrees with the geometry it measures is a property of one drawing, not a difference between two. The library reports a dimension's measured value and its text override as two separate fields, so a geometry change that left the text untouched is visible in the change set as exactly that; deciding that the drawing is now inconsistent is a summarizer's job, and the material for it is never lost here.
- **Not a judgement.** It says what changed and by how much, never whether the change is good.
- **Not a picture.** Nothing here renders or compares renders. Two drawings that render identically and differ by a tolerance are different.

## 7. What must be tested

The tests that keep this contract honest are of the "must never happen" kind, and their failure count is always zero:

- A known edit to one entity produces exactly one `MODIFIED` entry with exactly the fields that changed, and nothing else.
- Values at `tolerance - ε`, `tolerance` and `tolerance + ε` produce `WITHIN`, `BEYOND` and `BEYOND`.
- An injected wrong edit -- one that touches an entity it was not asked to -- always shows up as an additional entry.
- The same two states, compared twice, produce byte-identical output.
- Two revisions with no shared references: identical drawings produce an empty change set; one moved entity produces `REMOVED` plus `ADDED`; two identical entities that swapped places produce `UNKNOWN` with both candidates; two coincident entities of which one was deleted produce `UNKNOWN` for both.
- Changes no picture shows are reported: a move below the tolerance (as `WITHIN`), a move below any output grid but beyond the tolerance, one of two coincident entities deleted, a layer or colour change that keeps the same colour, a block edit cancelled by its instance's scale. Each has an exact expected change set.
