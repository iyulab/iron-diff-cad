//! The change set: what [`diff`](crate::diff) returns. Its serialized form
//! is the public contract (`docs/change-set.md`); the types here are that
//! contract as Rust.

use serde::{Deserialize, Serialize};
use uncad_model::model::{Confidence, EntityId, Origin};

/// Which key matched entities of the two states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum Matching {
    /// The two states share entity reference IDs, and those were the key,
    /// exactly as the model issued them.
    #[default]
    Reference,
    /// The two states share no references; entities were matched by type
    /// and shape within tolerance, and only where the match is certain.
    Geometry,
}

/// The tolerances a change set was computed with. Written into the change
/// set, so it cannot be read against different ones.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Tolerance {
    /// For every numeric field except angles, in the drawing's own units.
    pub length: f64,
    /// For fields the model documents as angles (radians).
    pub angle: f64,
}

impl Default for Tolerance {
    /// A visible default, not a hidden epsilon: it is written into every
    /// change set that used it.
    fn default() -> Self {
        Tolerance {
            length: 1e-6,
            angle: 1e-9,
        }
    }
}

/// Whether a numeric difference is within the tolerance or beyond it.
/// `|delta| < tolerance` is within; anything else, including exactly the
/// tolerance, is beyond.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Verdict {
    Within,
    Beyond,
}

/// One field of a modified entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldChange {
    /// The field's path in the model's JSON form (`radius`, `center.x`,
    /// `vertices[2].point.y`, `common.layer`).
    pub path: String,
    /// The value before, in the model's JSON form.
    pub before: serde_json::Value,
    /// The value after.
    pub after: serde_json::Value,
    /// `|after - before|` for a numeric field; absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta: Option<f64>,
    /// The tolerance applied to a numeric field; absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<f64>,
    /// For a numeric field, the tolerance verdict. A non-numeric field is
    /// listed only when it differs, and its verdict is always `Beyond`.
    pub verdict: Verdict,
}

/// An entity named by a change: its identity and markers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityRecord {
    pub id: EntityId,
    /// The DXF type name the model reports for the entity.
    pub entity_type: String,
    pub provenance: Origin,
    pub confidence: Confidence,
}

/// An entity present in both states with at least one differing field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Modified {
    /// The entity's reference ID in the first state.
    pub id: EntityId,
    /// Its counterpart's reference ID in the second state, when the two
    /// differ -- which is only under geometric matching, where the states
    /// share no references. Absent under reference matching, where the two
    /// are the same ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counterpart: Option<EntityId>,
    pub entity_type: String,
    /// The provenance of the entity before and after.
    pub provenance: [Origin; 2],
    /// The lower of the two entities' confidences: a change never carries a
    /// higher confidence than the entities it involves.
    pub confidence: Confidence,
    /// Every compared field that differs, including numeric fields that moved
    /// within tolerance, ordered by path.
    pub fields: Vec<FieldChange>,
}

/// An entity of the first state whose counterpart in the second could not
/// be decided with certainty. Only geometric matching produces it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Unknown {
    /// The entity's reference ID in the first state.
    pub id: EntityId,
    /// The entities of the second state that match it within tolerance,
    /// by their reference IDs, ascending. Never empty: an entity with no
    /// candidate is `REMOVED`.
    pub candidates: Vec<EntityId>,
    pub reason: String,
}

/// One verdict about one entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "UPPERCASE")]
pub enum Change {
    Added(EntityRecord),
    Removed(EntityRecord),
    Modified(Modified),
    Unknown(Unknown),
}

impl Change {
    /// The entity the change is about.
    pub fn id(&self) -> EntityId {
        match self {
            Change::Added(e) | Change::Removed(e) => e.id,
            Change::Modified(m) => m.id,
            Change::Unknown(u) => u.id,
        }
    }
}

/// The exact difference between two drawing states.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeSet {
    pub matching: Matching,
    pub tolerance: Tolerance,
    /// Ordered by entity reference ID, ascending, under reference matching;
    /// by entity type, representative point and second point under
    /// geometric matching (contract, section 4).
    pub changes: Vec<Change>,
}

impl ChangeSet {
    /// `true` when the two states were the same to within tolerance.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// The change set as JSON text, in the model's adjacent-tagging
    /// convention. Deterministic: the same change set gives the same bytes.
    pub fn to_json(&self, pretty: bool) -> Result<String, serde_json::Error> {
        if pretty {
            serde_json::to_string_pretty(self)
        } else {
            serde_json::to_string(self)
        }
    }
}
