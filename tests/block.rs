//! Which block definition a changed entity sits in. What a block reference
//! draws is named by the block, so a change of the entities inside a table's
//! or a dimension's block can be told from a change of the reference itself.

use iron_diff_cad::{diff, Change, DiffOptions, Matching};
use uncad_model::model::Entity;
use uncad_model::CadDatabase;

fn by_reference() -> DiffOptions {
    DiffOptions {
        matching: Matching::Reference,
        ..DiffOptions::default()
    }
}

/// The golden drawing: a plate with four holes, three linear dimensions and
/// a diameter dimension (each drawing its own anonymous block `*D1`..`*D4`)
/// and an INSERT of `TITLEBLOCK`.
fn g1() -> CadDatabase {
    serde_json::from_str(include_str!("golden/g1.expected.json"))
        .expect("the golden model deserializes")
}

fn block_of(change: &Change) -> Option<&str> {
    match change {
        Change::Added(e) | Change::Removed(e) => e.block.as_deref(),
        Change::Modified(m) => m.block.as_deref(),
        Change::Unknown(_) => panic!("reference matching decides every entity"),
    }
}

#[test]
fn an_entity_removed_from_a_block_names_the_block() {
    let before = g1();
    let mut after = g1();
    let gone = after
        .tables
        .block_records
        .get_mut("*D1")
        .unwrap()
        .entities
        .remove(0)
        .common()
        .id;

    let set = diff(&before, &after, by_reference());

    assert_eq!(set.changes.len(), 1);
    assert_eq!(set.changes[0].id(), gone);
    assert_eq!(block_of(&set.changes[0]), Some("*D1"));
}

#[test]
fn an_entity_added_to_a_block_names_the_block_of_the_second_state() {
    let before = g1();
    let mut after = g1();
    let mut copy = after.tables.block_records["*D1"].entities[0].clone();
    copy.common_mut().id = uncad_model::model::EntityId::new(9100);
    after
        .tables
        .block_records
        .get_mut("TITLEBLOCK")
        .unwrap()
        .entities
        .push(copy);

    let set = diff(&before, &after, by_reference());

    assert_eq!(set.changes.len(), 1);
    assert!(matches!(set.changes[0], Change::Added(_)));
    assert_eq!(block_of(&set.changes[0]), Some("TITLEBLOCK"));
}

#[test]
fn a_modified_entity_of_a_block_names_it_and_a_top_level_one_names_none() {
    let before = g1();
    let mut after = g1();
    // A top-level change: a hole's radius.
    let hole = after
        .entities
        .iter_mut()
        .find_map(|e| match e {
            Entity::Circle(c) => Some(c),
            _ => None,
        })
        .unwrap();
    hole.radius += 1.0;
    // A change inside a block: the first line a dimension draws.
    let inside = after
        .tables
        .block_records
        .get_mut("*D2")
        .unwrap()
        .entities
        .iter_mut()
        .find_map(|e| match e {
            Entity::Line(l) => Some(l),
            _ => None,
        })
        .expect("a dimension block holds a line");
    inside.end_point.x += 3.0;
    let inside_id = inside.common.id;

    let set = diff(&before, &after, by_reference());

    let blocks: Vec<(bool, Option<&str>)> = set
        .changes
        .iter()
        .map(|c| (c.id() == inside_id, block_of(c)))
        .collect();
    assert_eq!(blocks.len(), 2, "{blocks:?}");
    assert!(blocks.contains(&(true, Some("*D2"))), "{blocks:?}");
    assert!(blocks.contains(&(false, None)), "{blocks:?}");
}

#[test]
fn a_change_that_sits_in_no_block_carries_no_block_key() {
    let before = g1();
    let mut after = g1();
    let hole = after
        .entities
        .iter_mut()
        .find_map(|e| match e {
            Entity::Circle(c) => Some(c),
            _ => None,
        })
        .unwrap();
    hole.radius += 1.0;

    let json = diff(&before, &after, by_reference())
        .to_json(false)
        .unwrap();

    assert!(!json.contains("\"block\""), "{json}");
}
