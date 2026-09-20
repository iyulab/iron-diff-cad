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

Pre-implementation. No code yet. The design principles are settled and documented in [docs/principles.md](docs/principles.md); read that before proposing anything.

## License

MIT
