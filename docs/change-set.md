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
| `UNKNOWN` | The library could not decide which entity of the second state corresponds to this entity of the first. It lists the candidates (never empty: an entity with no candidate is `REMOVED`) and the reason. This is a normal result, not an error |

A confident wrong match would hide a real change, so an uncertain match is never resolved by picking the likeliest pair among rivals: a pair is taken only when the rules of section 3 single it out, and every rule and threshold that did is in the result.

### Fields of a `MODIFIED` entry

A `MODIFIED` entry carries the list of fields that were compared, one item per field path (`radius`, `center.x`, `layer`, `text_override`, ...):

- **Numeric fields**: `before`, `after`, `delta` (the absolute difference), the `tolerance` that was applied, and a `verdict` -- `WITHIN` or `BEYOND` (section 2). A field that moved *within* tolerance stays in the list: "it moved, but not beyond tolerance" is information, and hiding it would make it indistinguishable from "it did not move". A consumer that wants only the significant changes filters on the verdict.
- **Non-numeric fields** (names, text, flags): `before` and `after`, and they appear only when they differ. There is no tolerance and no delta.
- **A value one side does not state**: the model writes `null` where a file did not state a value, and documents per field what that means (a format older than the field, an optional group left out). When exactly one side holds that `null`, the field change also carries `unstated` -- `BEFORE` or `AFTER`, the side that did not state it. The verdict stays `BEYOND`: the two states do differ, and treating "not stated" as equal to some value would be inventing that value. The mark is what tells a drawing saved again in a newer format -- which states what the older format had no place for -- apart from an edit. A side that has no element there at all (an array that grew or shrank) is reported with `null` too but carries no mark: that is a change of shape.
- **Reference fields** (a layer, a block, a style -- three-state values in the model: resolved, absent, unresolved-with-handle): a change of the resolved value *or* of the state is a field change, reported with the two three-state values verbatim. A reference that stopped resolving is never swallowed as "no value".

Two fields are identity, not content, and are never compared: the reference ID (`common.id`) and the handle it was issued from (`common.source_handle`). Under reference matching they are the key; under geometric matching they differ for every pair by definition. Nor is a reference a save names rather than the drawing: a DIMENSION's `block_name` when both sides name an anonymous block (a name starting with `*`). The anonymous block a dimension is drawn with (`*D3`) is renumbered when the file is saved again while the dimension stays as it was, and what that block draws is what the dimension's own fields -- definition points, measurement, text, style -- already state; a dimension drawn differently differs in those, and the block's own entities are compared as entities. A named block is the drawing's own word: a dimension repointed to another named block, or between a named and an anonymous one, is a field change. An INSERT's `block_name` is different -- and an ACAD_TABLE's, whose cells are drawn only in its block: what such an entity draws is not in its own fields but in the block, so it is always compared -- and when both sides name an anonymous block (a dynamic block's current state, `*U24`), the change is left out exactly when the two blocks hold the same: the same number of entities, in the same order, each pair of the same type with no field beyond tolerance (identity aside, as everywhere), and the same base point. An entity of the blocks that is itself an INSERT of an anonymous block is held to the same rule, nested up to 20 deep; deeper, the names stay a change. Anonymous blocks that hold something else -- a dynamic block switched to another state -- keep the `block_name` change with both names, and the blocks' own entities are compared as entities. An IMAGE's `definition` names its image definition by handle -- identity, like the entity's own -- so it is held to the same rule whatever the handle: the change is left out exactly when the two image definitions state the same (file, size in pixels, pixel size, loaded, resolution unit, within tolerance), and an anonymous block's IMAGE is compared that way too.

Every entry also carries the **provenance** of both entities involved and a **confidence**, which is the lower of the two (a change between two low-confidence values is a low-confidence change, and no change is ever reported with a higher confidence than the entities it involves). Under geometric matching a `MODIFIED` entry also carries `counterpart`, the matched entity's reference ID in the second state, and -- when the two shapes differ -- `matched_by`, why they were taken as counterparts (section 3); under reference matching the two IDs are the same and both fields are absent.

### Projection

A change set is complete: it lists every field that differs, within tolerance or not. A caller that needs a smaller answer -- an agent reading it into a limited context, say -- takes a **projection** of it (`ChangeSet::without`), which leaves out the field changes it is asked to: those `WITHIN` tolerance, those one side does not state (`unstated`), or both. A `MODIFIED` entry none of whose fields remain is left out as well; `ADDED`, `REMOVED` and `UNKNOWN` entries are kept whole. A projection says what it left out, as counts in `omitted` (`within_fields`, `unstated_fields`, `entities`), so that an entity missing from it is never read as unchanged. A change set as the comparison returns it carries no `omitted`.

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
| `REFERENCE` | The two states share entity reference IDs (before and after an operation on the same model) | The reference ID, exactly as the model issued it -- this library defines no reference scheme of its own | A reference present in only one state is `ADDED` or `REMOVED`. A reference held by entities of different types in the two states is `REMOVED` plus `ADDED`: no operation on one model changes an entity's type, so they are two entities that carry the same ID, not one entity's field changes |
| `GEOMETRY` | The two states share no references (two revisions of a drawing) | Entity type plus shape: equal within tolerance first, then similar under stated thresholds | An uncertain correspondence is `UNKNOWN`; an entity with no candidate is `REMOVED` or `ADDED` |

The header names the mode that produced the change set, so the same output cannot be read in two meanings.

### Lineage, and choosing the mode

Reference matching is right only when the two states share their IDs because they are one drawing -- the same drawing edited or saved again. Two unrelated drawings share IDs by coincidence, and pairing those gives confident changes that are not there. So before matching, the two states' **lineage** is judged from what the files state, and the change set carries it (`lineage`) whatever the mode:

| Verdict | When |
|---|---|
| `DIFFERENT` | Both files state a `$FINGERPRINTGUID` and the two differ; or an ID both states hold names entities of different types, neither of them an entity the reader could not decode (`UNKNOWN`). Within one lineage an ID keeps its entity. |
| `SAME` | Not `DIFFERENT`; both files state the same `$FINGERPRINTGUID`; and the IDs both states hold are at least `threshold` of the smaller state's entities (by default half -- the caller may give another, and the change set states it). |
| `UNKNOWN` | Anything else: a fingerprint not stated, or too few IDs shared. |

The fingerprint is where a drawing started (the seed or template it was made from), not which drawing it is: unrelated drawings made from one template share it. A fingerprint that differs is therefore strong evidence of two lineages, one that is the same only weak evidence of one -- which is why `SAME` also asks for shared IDs. `lineage` states every fact the verdict was reached from: `fingerprint_equal` and `version_equal` (absent when either file does not state the GUID; a `$VERSIONGUID` that is the same as well means the same saved state), `shared`, `smaller`, `cross_type`, `threshold`.

The caller chooses the mode, or `AUTO` (the default): `REFERENCE` when the lineage is `SAME`, `GEOMETRY` otherwise -- geometric matching pairs entities only where its rules single a pair out, so a lineage that cannot be shown costs, at worst, a `MODIFIED` entry becoming `UNKNOWN` or `REMOVED` plus `ADDED` -- never a pair chosen among rivals. `matching` in the header is always the mode used, never `AUTO`. A caller that knows the two states are one drawing -- before and after an operation it made -- chooses `REFERENCE`, and `lineage` still tells whether the files bear that out.

### Geometric matching

An entity's **shape** is every field of its model form except the `common` block (identity, provenance, confidence, layer, colour), the type tag and a reference a save names (a DIMENSION's, an INSERT's or an ACAD_TABLE's `block_name` when it names an anonymous block, and an IMAGE's `definition` -- section 1; for all but the dimension, the pair is then held to the rule of section 1 when its fields are compared): the geometry and values that say what the entity *is*. A nested entity -- an INSERT's attributes -- is part of the shape without its own `common` block. A counterpart pair is compared on every field but identity: the reference ID and source handle are never compared fields, in the entity's `common` block or a nested entity's. Two entities have the same shape when they are of the same type and a field-by-field comparison (section 1, with the tolerance of section 2) finds no numeric field `BEYOND` and no non-numeric field different.

Matching runs in two stages, and every pair either stage takes is compared on every field (so a layer or colour change, or a move within tolerance, is a `MODIFIED` entry with `counterpart` set).

**Agreeing shapes.** An entity `x` of the first state and `y` of the second are counterparts when the match is certain both ways: `y` is the only entity of the second state with `x`'s shape, and `x` is the only entity of the first state with `y`'s. An entity with two or more such candidates, or with one candidate that is also another entity's only candidate, is `UNKNOWN` for the entity of the first state, with every candidate listed. Two coincident entities of which one was deleted are therefore two `UNKNOWN` entries, not a `REMOVED` and a match: which one went is not knowable from the geometry.

**Changed shapes.** The entities no shape agreed with -- in either state -- are then compared with the entities of their type left in the other state. A shape's **leaves** are the numbers, strings, booleans, `null`s and empty objects or arrays it holds, each under the path section 1 would report it by. A leaf path is **informative** unless every entity of that type, in both states and paired or not, holds it with one and the same value: such a leaf -- a flat drawing's `z` and extrusion, a radius every hole shares -- is a fact about the type in these drawings and tells no two entities apart. The **similarity** of two shapes is the share of the informative leaf paths either holds whose values agree: a path only one of them holds disagrees (so the longer shape sets the scale), a number agrees when it is within its tolerance (section 2), any other leaf when it is equal; with nothing informative to count it is 0. `x` and `y` are counterparts when all of these hold, under thresholds the caller states or the defaults:

1. their similarity is at least `min_similarity` (default `0.5`);
2. `y` is the single entity most similar to `x`, and `x` the single entity most similar to `y` -- a tie is no pair;
3. their similarity stands at least `min_margin` (default `0.1`) above the **runner-up**: the next highest similarity of `x` with another candidate, or of `y` with another entity of the first state, whichever is higher. With no runner-up the condition holds.

Such a `MODIFIED` entry carries `matched_by`: the `similarity` and the `runner_up` (absent when there was none). An entity of the first state that does not pair this way but has entities whose similarity reaches `min_similarity` is `UNKNOWN`, with those candidates, their `similarities` in the same order, and which condition failed as the reason; one with none is `REMOVED`. An entity of the second state is `ADDED` when it is neither a counterpart nor anyone's candidate. The thresholds are written into the header as `pairing`, so the same output cannot be read against different ones; a `min_similarity` above 1 pairs nothing in this stage, since a pair whose every leaf agrees has the same shape.

Scoring is quadratic in a type's left-over entities, so it is bounded by `max_pairs` in `pairing` (default ten million): types are taken in name order, and a type whose pairs do not fit in what is left of the bound is not scored. Its entities stay `REMOVED` and `ADDED`, and the header lists it in `unscored`, with the pairs it would have taken -- an entity is never left unpaired without the change set saying the stage did not look.

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
  "lineage": { "verdict": "SAME", "fingerprint_equal": true, "version_equal": false,
               "shared": 72, "smaller": 72, "cross_type": 0, "threshold": 0.5 },
  "changes": [
    { "type": "MODIFIED", "data": {
        "id": 289, "entity_type": "CIRCLE",
        "provenance": ["VECTOR", "DERIVED"], "confidence": "HIGH",
        "fields": [
          { "path": "radius", "before": 5.0, "after": 6.0, "delta": 1.0,
            "tolerance": 0.001, "verdict": "BEYOND" }
        ] } }
  ]
}
```

Under geometric matching the header also carries `pairing` (and `unscored` when a type was not scored), a pair whose shapes differ carries `matched_by`, and an `UNKNOWN` found by similarity carries `similarities`:

```json
{
  "matching": "GEOMETRY",
  "tolerance": { "length": 0.001, "angle": 0.0001 },
  "pairing": { "min_similarity": 0.5, "min_margin": 0.1, "max_pairs": 10000000 },
  "lineage": { "verdict": "UNKNOWN", "shared": 0, "smaller": 72, "cross_type": 0, "threshold": 0.5 },
  "changes": [
    { "type": "MODIFIED", "data": {
        "id": 289, "counterpart": 4385, "entity_type": "CIRCLE",
        "provenance": ["VECTOR", "VECTOR"], "confidence": "HIGH",
        "fields": [
          { "path": "radius", "before": 5.0, "after": 6.0, "delta": 1.0,
            "tolerance": 0.001, "verdict": "BEYOND" }
        ],
        "matched_by": { "similarity": 0.667, "runner_up": 0.333 } } },
    { "type": "UNKNOWN", "data": {
        "id": 301, "candidates": [412, 413],
        "reason": "2 entities of the second state are equally similar to this CIRCLE ...",
        "similarities": [0.5, 0.5] } }
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
- Two revisions with no shared references: identical drawings produce an empty change set; one moved or resized entity produces one `MODIFIED` entry with exactly the fields a comparison by reference reports, and `REMOVED` plus `ADDED` when nothing but agreeing shapes may pair; two identical entities that swapped places produce `UNKNOWN` with both candidates; two coincident entities of which one was deleted produce `UNKNOWN` for both; two changed entities equally similar to two others produce `UNKNOWN` for both, never a pair; two entities alike only in leaves every entity of their type shares are not paired; a type over the scoring bound is listed in `unscored`; the result does not depend on the order the entities are listed in.
- Changes no picture shows are reported: a move below the tolerance (as `WITHIN`), a move below any output grid but beyond the tolerance, one of two coincident entities deleted, a layer or colour change that keeps the same colour, a block edit cancelled by its instance's scale. Each has an exact expected change set.
