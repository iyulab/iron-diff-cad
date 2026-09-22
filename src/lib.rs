//! Deterministic numeric diff of two drawing states in the [`uncad_model`]
//! entity model.
//!
//! [`diff`] takes a *before* and an *after* [`CadDatabase`] that share entity
//! reference IDs (the state before and after an operation on the same
//! drawing) and returns the exact [`ChangeSet`] between them: which
//! entities were added, removed or modified, and for a modified one, every
//! field that differs, by how much, and whether that is within or beyond
//! the stated tolerance. The same two states produce the same change set,
//! in the same order, byte for byte.
//!
//! The shape of the change set is the contract in `docs/change-set.md`; the
//! rules are in `docs/principles.md`. Matching by geometry, for two
//! revisions that share no references, is not implemented yet: the
//! [`Matching::Geometry`] value exists so the header can name it.

#![forbid(unsafe_code)]

mod change_set;
mod fields;
mod reference;

pub use change_set::{
    Change, ChangeSet, EntityRecord, FieldChange, Matching, Modified, Tolerance, Unknown, Verdict,
};
pub use reference::{diff, DiffOptions};

pub use uncad_model::CadDatabase;
