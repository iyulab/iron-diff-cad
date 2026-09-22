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
//! Two revisions that share no references are compared with
//! [`Matching::Geometry`]: an entity's counterpart is the one entity of the
//! other state with the same type and shape within tolerance, and only when
//! that correspondence is certain both ways; anything less is reported as
//! [`Change::Unknown`] with the candidates, and a moved entity is
//! `REMOVED` plus `ADDED`.
//!
//! The shape of the change set is the contract in `docs/change-set.md`; the
//! rules are in `docs/principles.md`.

#![forbid(unsafe_code)]

mod change_set;
mod fields;
mod geometry;
mod reference;

pub use change_set::{
    Change, ChangeSet, EntityRecord, FieldChange, Matching, Modified, Tolerance, Unknown, Verdict,
};

pub use uncad_model::CadDatabase;

/// Options for [`diff`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DiffOptions {
    pub tolerance: Tolerance,
    /// How entities of the two states are paired. The default is by
    /// reference ID.
    pub matching: Matching,
}

/// The exact change set between `before` and `after`. Neither input is
/// modified; the result is a new value in the order the contract fixes for
/// the matching mode.
pub fn diff(before: &CadDatabase, after: &CadDatabase, options: DiffOptions) -> ChangeSet {
    match options.matching {
        Matching::Reference => reference::diff(before, after, options.tolerance),
        Matching::Geometry => geometry::diff(before, after, options.tolerance),
    }
}
