//! The same two states must produce the same change set, in the same order,
//! every time -- and the order must not depend on the order in which the
//! edits were made.

use iron_diff_cad::{diff, DiffOptions};
use uncad_model::model::{Entity, EntityId};
use uncad_model::CadDatabase;

/// How often each output is regenerated. With four or more entries in a
/// leaked hash set, two consecutive identical orders are already unlikely;
/// this many leave no realistic chance of a false pass.
const RUNS: usize = 24;

fn g1() -> CadDatabase {
    serde_json::from_str(include_str!("golden/g1.expected.json"))
        .expect("the golden model deserializes")
}

fn edit_every_hole(db: &mut CadDatabase, order: &[usize]) -> Vec<EntityId> {
    let ids: Vec<EntityId> = db
        .entities
        .iter()
        .filter_map(|e| match e {
            Entity::Circle(c) => Some(c.common.id),
            _ => None,
        })
        .collect();
    for &i in order {
        let id = ids[i];
        let blocks = db
            .tables
            .block_records
            .values_mut()
            .flat_map(|b| b.entities.iter_mut());
        for e in db.entities.iter_mut().chain(blocks) {
            if let Entity::Circle(c) = e {
                if c.common.id == id {
                    c.radius += 1.0 + i as f64;
                }
            }
        }
    }
    ids
}

#[test]
fn repeated_diffs_are_byte_identical() {
    let before = g1();
    let mut after = before.clone();
    edit_every_hole(&mut after, &[3, 1, 2, 0]);
    let first = diff(&before, &after, DiffOptions::default())
        .to_json(false)
        .unwrap();
    for run in 1..RUNS {
        let again = diff(&before, &after, DiffOptions::default())
            .to_json(false)
            .unwrap();
        assert_eq!(first, again, "run {run}: the change set changed");
    }
}

#[test]
fn the_order_is_by_reference_id_whatever_the_edit_order() {
    let before = g1();
    let mut a = before.clone();
    let ids = edit_every_hole(&mut a, &[3, 1, 2, 0]);
    let mut b = before.clone();
    edit_every_hole(&mut b, &[0, 1, 2, 3]);
    let set_a = diff(&before, &a, DiffOptions::default());
    let set_b = diff(&before, &b, DiffOptions::default());
    let order: Vec<EntityId> = set_a.changes.iter().map(|c| c.id()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(order, sorted, "ascending by reference ID");
    assert_eq!(
        set_b.changes.iter().map(|c| c.id()).collect::<Vec<_>>(),
        sorted
    );
}
