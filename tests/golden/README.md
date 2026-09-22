# Golden cases

`g1.expected.json` is a copy of what the `uncad-model` repository's golden writer produces
for the G1 spec: the model a reader must produce from the synthetic drawing. This crate uses
it as the *before* state of its diffs and makes the *after* states by editing it directly,
so what is tested is the diff alone -- no parser and no editor involved.

The file is generated, not hand-written. To regenerate after a change to the writer or the
spec, from a checkout of `uncad-model`:

```
cargo run -p uncad-model-golden --example write_case -- g1 g1.dxf g1.expected.json
```

and copy the JSON here. A tree that carries both repositories side by side checks that the
copy has not drifted from the writer.
