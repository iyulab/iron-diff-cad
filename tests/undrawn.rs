//! Changes inside block definitions nothing draws. Such a definition still
//! holds its entities, and a save may drop it; the change set reports that
//! as the change of the model it is, and marks it `undrawn` so a reader
//! can tell it from a change any picture shows.

use iron_diff_cad::{diff, Change, DiffOptions, Matching};
use uncad_model::model::{Entity, EntityId, Ref};
use uncad_model::tables::BlockRecord;
use uncad_model::CadDatabase;

fn by_reference() -> DiffOptions {
    DiffOptions {
        matching: Matching::Reference,
        ..DiffOptions::default()
    }
}

/// The golden drawing: three dimensions drawing `*D1`..`*D3` (and `*D4`)
/// and an INSERT of `TITLEBLOCK`.
fn g1() -> CadDatabase {
    serde_json::from_str(include_str!("golden/g1.expected.json"))
        .expect("the golden model deserializes")
}

/// A copy of block `from` under `name`, its entities given fresh IDs from
/// `first_id` on, referred to by nothing.
fn orphan_copy(db: &mut CadDatabase, from: &str, name: &str, first_id: u64) -> Vec<EntityId> {
    let mut block: BlockRecord = db.tables.block_records[from].clone();
    block.name = name.to_string();
    let mut ids = Vec::new();
    for (i, e) in block.entities.iter_mut().enumerate() {
        let id = EntityId::new(first_id + i as u64);
        e.common_mut().id = id;
        ids.push(id);
    }
    db.tables.block_records.insert(name.to_string(), block);
    ids
}

fn block_entity_ids(db: &CadDatabase, name: &str) -> Vec<EntityId> {
    db.tables.block_records[name]
        .entities
        .iter()
        .map(|e| e.common().id)
        .collect()
}

/// Every change about one of `ids`, and whether each is marked undrawn.
fn marks(changes: &[Change], ids: &[EntityId]) -> Vec<(EntityId, bool)> {
    changes
        .iter()
        .filter(|c| ids.contains(&c.id()))
        .map(|c| {
            let undrawn = match c {
                Change::Added(e) | Change::Removed(e) => e.undrawn,
                Change::Modified(m) => m.undrawn,
                Change::Unknown(_) => panic!("reference matching decides every entity"),
            };
            (c.id(), undrawn)
        })
        .collect()
}

#[test]
fn a_block_nothing_draws_dropped_by_a_save_is_removed_and_undrawn() {
    let mut before = g1();
    let ids = orphan_copy(&mut before, "*D1", "*D90", 9000);
    let after = g1();

    let set = diff(&before, &after, by_reference());

    assert_eq!(
        set.changes.len(),
        ids.len(),
        "only the orphan's entities changed"
    );
    for (id, undrawn) in marks(&set.changes, &ids) {
        assert!(undrawn, "entity {id:?} of a block nothing draws is undrawn");
    }
    assert!(set.changes.iter().all(|c| matches!(c, Change::Removed(_))));
}

#[test]
fn an_entity_removed_from_a_drawn_block_is_not_undrawn() {
    let before = g1();
    let mut after = g1();
    let block = after.tables.block_records.get_mut("*D1").unwrap();
    let gone = block.entities.remove(0).common().id;

    let set = diff(&before, &after, by_reference());

    assert_eq!(marks(&set.changes, &[gone]), vec![(gone, false)]);
    let json = set.to_json(false).unwrap();
    assert!(
        !json.contains("undrawn"),
        "a drawn change carries no mark: {json}"
    );
}

#[test]
fn a_block_reached_only_through_a_block_nothing_draws_is_undrawn_too() {
    let mut before = g1();
    // `*D91` is referred to only by an INSERT inside `*D90`, which nothing draws.
    let inner = orphan_copy(&mut before, "*D2", "*D91", 9100);
    orphan_copy(&mut before, "*D1", "*D90", 9000);
    let mut insert = before
        .entities
        .iter()
        .find(|e| e.type_name() == "INSERT")
        .expect("g1 has an INSERT")
        .clone();
    insert.common_mut().id = EntityId::new(9050);
    if let Entity::Insert(i) = &mut insert {
        i.block_name = Ref::Resolved("*D91".to_string());
        i.attribs.clear();
    }
    before
        .tables
        .block_records
        .get_mut("*D90")
        .unwrap()
        .entities
        .push(insert);
    let after = g1();

    let set = diff(&before, &after, by_reference());

    for (id, undrawn) in marks(&set.changes, &inner) {
        assert!(
            undrawn,
            "entity {id:?} reached only through an orphan is undrawn"
        );
    }
}

#[test]
fn an_edit_inside_a_block_nothing_draws_is_modified_and_undrawn() {
    let mut before = g1();
    let ids = orphan_copy(&mut before, "*D1", "*D90", 9000);
    let mut after = before.clone();
    let entity = after.tables.block_records.get_mut("*D90").unwrap().entities[0].clone();
    let mut moved = entity;
    if let Entity::Line(l) = &mut moved {
        l.start_point.x += 1.0;
    } else {
        panic!("the first entity of *D1 is a LINE");
    }
    after.tables.block_records.get_mut("*D90").unwrap().entities[0] = moved;

    let set = diff(&before, &after, by_reference());

    assert_eq!(marks(&set.changes, &ids), vec![(ids[0], true)]);
    assert!(matches!(set.changes[0], Change::Modified(_)));
    assert!(set.to_json(false).unwrap().contains(r#""undrawn":true"#));
}

#[test]
fn an_edit_inside_a_block_one_state_draws_is_not_undrawn() {
    let mut before = g1();
    let ids = orphan_copy(&mut before, "*D1", "*D90", 9000);
    let mut after = before.clone();
    // The second state draws `*D90`: its INSERT -- listed at the top level
    // and under `*Model_Space` -- now refers to it.
    let model_space = after.tables.block_records.get_mut("*Model_Space").unwrap();
    for e in after
        .entities
        .iter_mut()
        .chain(model_space.entities.iter_mut())
    {
        if let Entity::Insert(i) = e {
            i.block_name = Ref::Resolved("*D90".to_string());
        }
    }
    if let Entity::Line(l) = &mut after.tables.block_records.get_mut("*D90").unwrap().entities[0] {
        l.start_point.x += 1.0;
    }

    let set = diff(&before, &after, by_reference());

    assert_eq!(marks(&set.changes, &ids), vec![(ids[0], false)]);
}

#[test]
fn the_golden_drawing_draws_every_block_entity_it_holds() {
    // Every block of g1 is drawn: an edit of each block's first entity is
    // never undrawn.
    let before = g1();
    for name in ["*D1", "*D2", "*D3", "*D4", "TITLEBLOCK"] {
        let ids = block_entity_ids(&before, name);
        let mut after = before.clone();
        after
            .tables
            .block_records
            .get_mut(name)
            .unwrap()
            .entities
            .remove(0);
        let set = diff(&before, &after, by_reference());
        assert_eq!(
            marks(&set.changes, &ids[..1]),
            vec![(ids[0], false)],
            "{name}"
        );
    }
}
