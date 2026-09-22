# iron-diff-cad

Deterministic numeric diff for CAD drawings: takes two drawing states and returns the exact set of changes between them — what was added, removed, or modified, and by how much.

Built as a tool to be handed to an agent. It contains no AI of its own, and it is just as useful without an agent.

## What it does

- **Verify an edit** — two states that share entity references (before and after an operation) → the precise change set. This is the accepted evidence that an edit did what was intended.
- **Compare revisions** — two independent drawings with no shared references → entities matched by geometry, then diffed.

Input and output are expressed in the [uncad-model](https://github.com/iyulab/uncad-model) entity model. Rendering a change set as an overlay is the job of [iron-render-cad](https://github.com/iyulab/iron-render-cad).

## What it is not

- Not an image comparison. It never looks at pixels; two drawings that render identically but differ by a tolerance are different.
- Not a file parser and not a renderer.
- Not an ML library. It performs no inference. Matching is geometric and exact within a stated numeric tolerance.
- Not a judge. It reports what changed, not whether the change is good.

## Status

0.x. `diff(&before, &after, options)` matches two states by their entity reference IDs (`Matching::Reference`, the default) or, for two revisions that share no references, by entity type and shape within tolerance (`Matching::Geometry`, where only a match that is certain both ways counts and everything else is `UNKNOWN` with its candidates), and returns the exact change set -- added, removed and modified entities, every differing field of a modified one with its delta and a within/beyond verdict against an explicit tolerance -- in a fixed order, byte for byte the same every time. The rules are in [docs/principles.md](docs/principles.md); the shape of what comes back is the contract in [docs/change-set.md](docs/change-set.md). Read both before proposing anything.

```rust
let before: uncad_model::CadDatabase = /* from a parser, or from its JSON */;
let after = /* the same drawing after an operation */;
let set = iron_diff_cad::diff(&before, &after, iron_diff_cad::DiffOptions::default());
for change in &set.changes {
    println!("{}", serde_json::to_string(change)?);
}
```

## License

MIT
