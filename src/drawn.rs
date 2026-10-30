//! Which entities a state draws.
//!
//! A block definition's entities are drawn only through a reference to the
//! block -- an INSERT, a dimension's anonymous block, a table's, a
//! multileader's content block -- from something that is itself drawn. A
//! definition nothing draws still holds its entities, and a save may drop
//! it (anonymous dimension blocks no dimension points at any longer are
//! routinely cleaned up when a drawing is saved again), so a change inside
//! it is a true change of the model that no picture of the drawing shows.
//! That is what is computed here: the entities of one state that sit only
//! in definitions nothing drawn refers to.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use uncad_model::model::{Entity, EntityId};
use uncad_model::CadDatabase;

/// The reference IDs of the entities `db` holds only in block definitions
/// nothing drawn refers to, directly or through other blocks.
///
/// What is drawn starts at the drawing's own entities and the layout blocks
/// (`*Model_Space`, `*Paper_Space…`), and follows every resolved
/// `block_name` reference in a drawn entity's model form to that block's
/// entities. A reference names its block exactly as the file does; a name
/// no block record holds draws nothing.
pub(crate) fn undrawn(db: &CadDatabase) -> BTreeSet<EntityId> {
    let mut drawn_blocks: BTreeSet<&str> = BTreeSet::new();
    let mut queue: Vec<&Entity> = db.entities.iter().collect();
    for (name, block) in &db.tables.block_records {
        if is_layout(name) {
            drawn_blocks.insert(name.as_str());
            queue.extend(block.entities.iter());
        }
    }

    let mut drawn: BTreeSet<EntityId> = BTreeSet::new();
    while let Some(entity) = queue.pop() {
        if !drawn.insert(entity.common().id) {
            continue;
        }
        let value = serde_json::to_value(entity).expect("the model serializes");
        let mut names = Vec::new();
        block_names(&value, &mut names);
        for name in names {
            if let Some((key, block)) = db.tables.block_records.get_key_value(name) {
                if drawn_blocks.insert(key.as_str()) {
                    queue.extend(block.entities.iter());
                }
            }
        }
    }

    db.tables
        .block_records
        .values()
        .flat_map(|b| b.entities.iter())
        .map(|e| e.common().id)
        .filter(|id| !drawn.contains(id))
        .collect()
}

/// The block definition each entity of `db` sits in, for the entities that
/// sit in one that is not a layout. An entity of the drawing's own list, or
/// of a layout, has none.
pub(crate) fn blocks(db: &CadDatabase) -> BTreeMap<EntityId, String> {
    let layout: BTreeSet<EntityId> = db
        .tables
        .block_records
        .iter()
        .filter(|(name, _)| is_layout(name))
        .flat_map(|(_, block)| block.entities.iter().map(|e| e.common().id))
        .chain(db.entities.iter().map(|e| e.common().id))
        .collect();
    db.tables
        .block_records
        .iter()
        .filter(|(name, _)| !is_layout(name))
        .flat_map(|(name, block)| {
            block
                .entities
                .iter()
                .map(move |e| (e.common().id, name.clone()))
        })
        .filter(|(id, _)| !layout.contains(id))
        .collect()
}

/// `*Model_Space` and every `*Paper_Space…`: the blocks a drawing's layouts
/// are, drawn without any reference. Block names compare without regard to
/// case, as the format's do.
fn is_layout(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper == "*MODEL_SPACE" || upper.starts_with("*PAPER_SPACE")
}

/// The block names `value` refers to: every `block_name` field, at any
/// depth, whose reference is resolved.
fn block_names<'v>(value: &'v Value, out: &mut Vec<&'v str>) {
    match value {
        Value::Object(map) => {
            for (key, field) in map {
                if key == "block_name" && field["type"] == "RESOLVED" {
                    if let Some(name) = field["data"].as_str() {
                        out.push(name);
                    }
                } else {
                    block_names(field, out);
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| block_names(item, out)),
        _ => {}
    }
}
