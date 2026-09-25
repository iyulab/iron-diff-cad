//! Matching by reference ID: the two states share the model's entity IDs,
//! and those are the key, exactly as issued.

use crate::change_set::{Change, ChangeSet, EntityRecord, Matching, Modified, Tolerance};
use crate::fields;
use std::collections::BTreeMap;
use uncad_model::model::{Entity, EntityId};
use uncad_model::CadDatabase;

/// Every entity of a drawing, by reference ID: the drawing's own entities
/// and every block definition's. An entity that appears in both places (a
/// model-space entity is also listed under its block) is one entity.
pub(crate) fn by_id(db: &CadDatabase) -> BTreeMap<EntityId, &Entity> {
    let mut map = BTreeMap::new();
    let in_blocks = db
        .tables
        .block_records
        .values()
        .flat_map(|b| b.entities.iter());
    for e in db.entities.iter().chain(in_blocks) {
        map.entry(e.common().id).or_insert(e);
    }
    map
}

pub(crate) fn record(e: &Entity) -> EntityRecord {
    EntityRecord {
        id: e.common().id,
        entity_type: e.type_name().to_string(),
        provenance: e.common().origin,
        confidence: e.common().confidence,
    }
}

/// The exact change set between `before` and `after`, matched by reference
/// ID. Neither input is modified; the result is a new value, ordered by ID.
pub fn diff(before: &CadDatabase, after: &CadDatabase, tolerance: Tolerance) -> ChangeSet {
    let b = by_id(before);
    let a = by_id(after);
    let mut ids: Vec<EntityId> = b.keys().chain(a.keys()).copied().collect();
    ids.sort();
    ids.dedup();

    let mut changes = Vec::new();
    for id in ids {
        match (b.get(&id), a.get(&id)) {
            (Some(x), None) => changes.push(Change::Removed(record(x))),
            (None, Some(y)) => changes.push(Change::Added(record(y))),
            (Some(x), Some(y)) => {
                if x == y {
                    continue;
                }
                let bx = serde_json::to_value(x).expect("the model serializes");
                let ay = serde_json::to_value(y).expect("the model serializes");
                let fields = fields::compare(&bx, &ay, tolerance);
                if fields.is_empty() {
                    // Different as Rust values but not as compared fields
                    // (e.g. a non-finite float): nothing to report.
                    continue;
                }
                changes.push(Change::Modified(Modified {
                    id,
                    counterpart: None,
                    entity_type: y.type_name().to_string(),
                    provenance: [x.common().origin, y.common().origin],
                    confidence: x.common().confidence.min(y.common().confidence),
                    fields,
                }));
            }
            (None, None) => unreachable!("an id came from one of the two maps"),
        }
    }

    ChangeSet {
        matching: Matching::Reference,
        tolerance,
        changes,
        omitted: None,
    }
}
