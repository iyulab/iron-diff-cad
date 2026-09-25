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

/// Which of the two states a field change refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Side {
    Before,
    After,
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
    /// The state whose value at this path is the model's `null` -- the
    /// file did not state it, with the meaning the model documents for that
    /// field -- when the other state's is not. A drawing saved again in a
    /// newer format states values the older one had no place for, and this
    /// is how such a change tells itself apart from an edit. Absent when
    /// both sides carry a value, and when one side has no such element at
    /// all (an array grew or shrank): that is a change of shape, not of what
    /// was stated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unstated: Option<Side>,
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
    /// What a projection ([`ChangeSet::without`]) left out, counted. Absent
    /// on a change set as [`crate::diff`] returns it, which leaves nothing
    /// out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub omitted: Option<Omitted>,
}

/// Which field changes a projection of a change set leaves out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Omit {
    /// Numeric fields that moved within tolerance.
    pub within: bool,
    /// Fields one side does not state ([`FieldChange::unstated`]).
    pub unstated: bool,
}

/// What a projection left out, so that "not listed" is never mistaken for
/// "did not change".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Omitted {
    /// Field changes left out because they were within tolerance.
    pub within_fields: usize,
    /// Field changes left out because one side does not state the value.
    pub unstated_fields: usize,
    /// `MODIFIED` entries left out because none of their fields remained.
    pub entities: usize,
}

impl ChangeSet {
    /// `true` when the two states were the same to within tolerance.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// This change set with the field changes `omit` names left out, and a
    /// `MODIFIED` entry left out when none of its fields remain -- counted in
    /// [`Self::omitted`], added to whatever an earlier projection counted.
    /// `ADDED`, `REMOVED` and `UNKNOWN` entries are kept whole. `self` is not
    /// modified.
    pub fn without(&self, omit: Omit) -> ChangeSet {
        let mut omitted = self.omitted.unwrap_or_default();
        let mut changes = Vec::with_capacity(self.changes.len());
        for change in &self.changes {
            let Change::Modified(m) = change else {
                changes.push(change.clone());
                continue;
            };
            let mut fields = Vec::with_capacity(m.fields.len());
            for f in &m.fields {
                if omit.within && f.verdict == Verdict::Within {
                    omitted.within_fields += 1;
                } else if omit.unstated && f.unstated.is_some() {
                    omitted.unstated_fields += 1;
                } else {
                    fields.push(f.clone());
                }
            }
            if fields.is_empty() {
                omitted.entities += 1;
            } else {
                changes.push(Change::Modified(Modified {
                    fields,
                    ..m.clone()
                }));
            }
        }
        ChangeSet {
            matching: self.matching,
            tolerance: self.tolerance,
            changes,
            omitted: Some(omitted),
        }
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
