//! Whether two drawing states are of one lineage -- the same drawing, saved
//! again or edited -- judged only from what the two files state: the
//! header's `$FINGERPRINTGUID` and `$VERSIONGUID`, and the reference IDs
//! the states share.
//!
//! Reference matching is right only when the states share their IDs because
//! they are one drawing; two unrelated drawings share IDs by coincidence,
//! and pairing those gives confident changes that are not there. The
//! verdict says which case the files show, and says `UNKNOWN` rather than
//! guess:
//!
//! - `DIFFERENT` when both state a fingerprint and the two differ, or when
//!   an ID both states hold names entities of different types (neither of
//!   them one the reader could not decode). Within one lineage an ID keeps
//!   its entity; a different type under it is another drawing.
//! - `SAME` when it is not `DIFFERENT`, both state the same fingerprint,
//!   and the IDs both hold are at least `threshold` of the smaller state's
//!   entities.
//! - `UNKNOWN` otherwise -- a fingerprint not stated, or too few IDs shared.
//!
//! The fingerprint is where a drawing started (the seed or template it was
//! made from), not which drawing it is: unrelated drawings made from one
//! template share it. So a fingerprint that differs is strong evidence of
//! two lineages, and one that is the same is weak evidence of one -- which
//! is why `SAME` asks for shared IDs too.

use serde::{Deserialize, Serialize};
use uncad_model::model::Entity;
use uncad_model::CadDatabase;

use crate::reference::by_id;

/// The share of the smaller state's entities whose IDs both states must
/// hold for `SAME`, unless the caller gives another.
pub const DEFAULT_SHARED_THRESHOLD: f64 = 0.5;

/// What the two files show about their lineage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LineageVerdict {
    Same,
    Different,
    Unknown,
}

/// The lineage verdict and every fact it was reached from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Lineage {
    pub verdict: LineageVerdict,
    /// Whether the two `$FINGERPRINTGUID`s are the same; absent when either
    /// file does not state one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint_equal: Option<bool>,
    /// Whether the two `$VERSIONGUID`s are the same -- the same saved state,
    /// when the lineage is too; absent when either file does not state one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_equal: Option<bool>,
    /// How many reference IDs both states hold.
    pub shared: usize,
    /// How many entities the smaller state holds: what `shared` is measured
    /// against.
    pub smaller: usize,
    /// How many of the shared IDs name entities of different types, neither
    /// of them one the reader could not decode.
    pub cross_type: usize,
    /// The share of `smaller` that `shared` had to reach for `SAME`.
    pub threshold: f64,
}

/// The lineage of `before` and `after`, with `threshold` the share of the
/// smaller state's entities the shared IDs must reach for `SAME`.
pub fn lineage(before: &CadDatabase, after: &CadDatabase, threshold: f64) -> Lineage {
    let equal = |a: &Option<String>, b: &Option<String>| match (a, b) {
        (Some(a), Some(b)) => Some(a.eq_ignore_ascii_case(b)),
        _ => None,
    };
    let fingerprint_equal = equal(
        &before.header.fingerprintguid,
        &after.header.fingerprintguid,
    );
    let version_equal = equal(&before.header.versionguid, &after.header.versionguid);
    let (b, a) = (by_id(before), by_id(after));
    let mut shared = 0;
    let mut cross_type = 0;
    for (id, x) in &b {
        let Some(y) = a.get(id) else { continue };
        shared += 1;
        let undecoded = |e: &Entity| matches!(e, Entity::Unknown { .. });
        if x.type_name() != y.type_name() && !undecoded(x) && !undecoded(y) {
            cross_type += 1;
        }
    }
    let smaller = b.len().min(a.len());
    let verdict = if fingerprint_equal == Some(false) || cross_type > 0 {
        LineageVerdict::Different
    } else if fingerprint_equal == Some(true) && shared as f64 >= threshold * smaller as f64 {
        LineageVerdict::Same
    } else {
        LineageVerdict::Unknown
    };
    Lineage {
        verdict,
        fingerprint_equal,
        version_equal,
        shared,
        smaller,
        cross_type,
        threshold,
    }
}
