# Principles

> This document describes the rules currently in force. A sentence here that is wrong is a bug.

## 1. Determinism is non-negotiable

The same two states produce the same change set, in the same order. The library never calls an inference endpoint, never embeds a model, and never guesses.

When two entities cannot be matched with certainty, the library says so — it reports the candidates as **unmatched** or the match as **"unknown"** rather than picking the likeliest pair. "Unknown" is a normal result, not an error.

*What this costs:* a revision comparison will sometimes report "removed + added" where a human would see "moved". That is intended — a confident wrong match hides a real change.

## 2. Numbers, not pictures

A difference is a numeric fact about the model: this radius went from 5.000 to 5.010. The library never rasterizes and never compares renders. Tolerances are invisible in a render, and "looks the same" is not "is the same".

Every comparison uses an explicit numeric tolerance, stated by the caller or defaulted visibly. There is no hidden epsilon.

## 3. The input is never modified

Both states are read-only. The change set is a new value.

## 4. Provenance and confidence travel with every change

The entity model carries a **reference ID**, a **provenance** and a **confidence** for every entity. A reported change carries the confidence of the entities it involves, and never a higher one. A change between two low-confidence values is a low-confidence change.

## 5. One reference scheme

When the two states share entity references, those references are the matching key, exactly as issued. The library defines no reference scheme of its own. Geometric matching is used only when no shared references exist, and the result says which mode produced it.

## 6. Domain neutrality

The library knows CAD entities and numbers. It does not know what a change *means* — whether it is a cost reduction, a violation, or an improvement is the caller's business. Concepts that only one consumer needs belong in that consumer's adapter, including when they are dressed in generic-sounding names.

## 7. Trade-off order

When goals collide, the earlier one wins:

> API simplicity › coverage › development speed › backward compatibility

Determinism is not on this list because it is never traded. Licensing is not on this list because it is a constraint: the dependency tree of this crate is permissive-only (MIT / Apache-2.0 / BSD).

## 8. Compatibility

The crate is in 0.x. When a more correct design is found, a breaking change is the normal way to adopt it; it is not deferred for migration cost. Breaking changes bump the minor version. The major version is not bumped without an explicit maintainer decision.

## 9. Contributing

| Just do it | Propose first | Discuss before any work |
|---|---|---|
| Tests · bug fixes and refactors that leave the public API unchanged · docs · a new field on a result the library returns | Public API changes · new dependencies · changes to the shape of the change set other than a new field | Anything that adds inference · anything in "What it is not" · copyleft dependencies · changes to how confidence is carried |

If it is unclear which column a change falls in, treat it as the stricter one.

Tests for things the library **must not do** — missing a real change, reporting a change that did not happen, raising a confidence, modifying its input — are required, and their failure count is always zero. Mutation tests (inject a known wrong edit, require that it is reported) are the primary way this is checked.
