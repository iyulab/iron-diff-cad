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
| `UNKNOWN` | The library could not decide, with certainty, which entity of the second state (if any) corresponds to this one. It lists the candidates and the reason. This is a normal result, not an error |

A confident wrong match would hide a real change, so an uncertain match is never resolved by picking the likeliest pair. Where a human would see "moved", a comparison without shared references reports `REMOVED` plus `ADDED`; that is the intended cost.

### Fields of a `MODIFIED` entry

A `MODIFIED` entry carries the list of fields that were compared, one item per field path (`radius`, `center.x`, `layer`, `text_override`, ...):

- **Numeric fields**: `before`, `after`, `delta` (the absolute difference), the `tolerance` that was applied, and a `verdict` -- `WITHIN` or `BEYOND` (section 2). A field that moved *within* tolerance stays in the list: "it moved, but not beyond tolerance" is information, and hiding it would make it indistinguishable from "it did not move". A consumer that wants only the significant changes filters on the verdict.
- **Non-numeric fields** (names, text, flags): `before` and `after`, and they appear only when they differ. There is no tolerance and no delta.
- **Reference fields** (a layer, a block, a style -- three-state values in the model: resolved, absent, unresolved-with-handle): a change of the resolved value *or* of the state is a field change, reported with the two three-state values verbatim. A reference that stopped resolving is never swallowed as "no value".

Every entry also carries the **provenance** of both entities involved and a **confidence**, which is the lower of the two (a change between two low-confidence values is a low-confidence change, and no change is ever reported with a higher confidence than the entities it involves).

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

## 4. Order

The same two states produce the same change set in the same order, byte for byte. The ordering keys are part of the contract:

- In `REFERENCE` mode, entries are ordered by reference ID, ascending.
- In `GEOMETRY` mode, entries are ordered by entity type, then by the entity's representative point compared lexicographically on (x, y, z), then by its second point where the type has one.
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
- Two revisions with no shared references: identical drawings produce an empty change set; one moved entity produces `REMOVED` plus `ADDED`; two identical entities that swapped places produce `UNKNOWN` with both candidates.
