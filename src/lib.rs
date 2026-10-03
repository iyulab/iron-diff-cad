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
//! Whether the two states are one drawing at all is judged first, from what
//! the files state ([`lineage`]); under the default [`Matching::Auto`] that
//! verdict picks the mode, and the change set carries it either way.
//!
//! Two revisions that share no references are compared with
//! [`Matching::Geometry`]: an entity's counterpart is the one entity of the
//! other state with the same type and shape within tolerance, and only when
//! that correspondence is certain both ways. An entity whose shape changed
//! is then paired with the one entity of its type it is most similar to,
//! only when stated thresholds ([`Pairing`]) single that pair out both ways;
//! anything less is reported as [`Change::Unknown`] with the candidates,
//! and an entity with none is `REMOVED` or `ADDED`.
//!
//! The shape of the change set is the contract in `docs/change-set.md`; the
//! rules are in `docs/principles.md`.

#![forbid(unsafe_code)]

mod change_set;
mod fields;
mod geometry;
pub mod lineage;
mod reference;

pub use change_set::{
    Change, ChangeSet, EntityRecord, FieldChange, MatchedBy, Matching, Modified, Omit, Omitted,
    Pairing, Side, Tolerance, Unknown, Verdict,
};
pub use lineage::{lineage, Lineage, LineageVerdict, DEFAULT_SHARED_THRESHOLD};

pub use uncad_model::CadDatabase;

/// Options for [`diff`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffOptions {
    pub tolerance: Tolerance,
    /// How entities of the two states are paired. The default,
    /// [`Matching::Auto`], lets the lineage decide; a caller that knows the
    /// two states are one drawing -- before and after an edit it made --
    /// says [`Matching::Reference`].
    pub matching: Matching,
    /// The share of the smaller state's entities whose IDs both states must
    /// hold for the lineage to be `SAME` ([`lineage`]).
    pub shared_threshold: f64,
    /// When geometric matching pairs entities whose shapes differ.
    pub pairing: Pairing,
}

impl Default for DiffOptions {
    fn default() -> Self {
        DiffOptions {
            tolerance: Tolerance::default(),
            matching: Matching::Auto,
            shared_threshold: DEFAULT_SHARED_THRESHOLD,
            pairing: Pairing::default(),
        }
    }
}

/// The exact change set between `before` and `after`. Neither input is
/// modified; the result is a new value in the order the contract fixes for
/// the matching mode.
pub fn diff(before: &CadDatabase, after: &CadDatabase, options: DiffOptions) -> ChangeSet {
    let lineage = lineage(before, after, options.shared_threshold);
    let matching = match options.matching {
        Matching::Auto if lineage.verdict == LineageVerdict::Same => Matching::Reference,
        Matching::Auto => Matching::Geometry,
        chosen => chosen,
    };
    let mut set = match matching {
        Matching::Geometry => geometry::diff(before, after, options.tolerance, options.pairing),
        // `Auto` was resolved above.
        Matching::Reference | Matching::Auto => reference::diff(before, after, options.tolerance),
    };
    set.lineage = Some(lineage);
    set
}
