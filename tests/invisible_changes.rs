//! Changes no picture can show. Each *after* state here draws the same
//! image as its *before* state -- by construction, not by rendering
//! anything: a move smaller than any output grid, one of two coincident
//! entities gone, a layer or colour change that lands on the same colour,
//! a block edit cancelled by its instance's scale. A comparison of renders
//! passes every one of them; the change set does not, and each test pins
//! exactly what it reports.

use iron_diff_cad::{diff, Change, DiffOptions, Matching, Verdict};
use uncad_model::model::{Entity, EntityId, Ref};
use uncad_model::CadDatabase;

fn g1() -> CadDatabase {
    serde_json::from_str(include_str!("golden/g1.expected.json"))
        .expect("the golden model deserializes")
}

fn g6() -> CadDatabase {
    serde_json::from_str(include_str!("golden/g6.expected.json"))
        .expect("the golden model deserializes")
}

fn geometry() -> DiffOptions {
    DiffOptions {
        matching: Matching::Geometry,
        ..DiffOptions::default()
    }
}

/// Every occurrence of the entity with `id` (top level and its block).
fn entity_mut(db: &mut CadDatabase, id: EntityId) -> Vec<&mut Entity> {
    let blocks = db
        .tables
        .block_records
        .values_mut()
        .flat_map(|b| b.entities.iter_mut());
    let hits: Vec<&mut Entity> = db
        .entities
        .iter_mut()
        .chain(blocks)
        .filter(|e| e.common().id == id)
        .collect();
    assert!(!hits.is_empty(), "entity {id:?} exists");
    hits
}

fn first_of(db: &CadDatabase, pick: impl Fn(&Entity) -> bool) -> EntityId {
    db.entities
        .iter()
        .find(|e| pick(e))
        .map(|e| e.common().id)
        .expect("the entity exists")
}

fn kinds(set: &iron_diff_cad::ChangeSet) -> Vec<&'static str> {
    set.changes
        .iter()
        .map(|c| match c {
            Change::Added(_) => "ADDED",
            Change::Removed(_) => "REMOVED",
            Change::Modified(_) => "MODIFIED",
            Change::Unknown(_) => "UNKNOWN",
        })
        .collect()
}

fn field_paths(change: &Change) -> Vec<&str> {
    match change {
        Change::Modified(m) => m.fields.iter().map(|f| f.path.as_str()).collect(),
        other => panic!("expected MODIFIED, got {other:?}"),
    }
}

/// A move of one ten-millionth of a unit: below the default tolerance and
/// far below any output grid (a 2400 dpi PDF quantizes to about 0.01 of a
/// unit at typical scales). Reported, as `WITHIN`.
#[test]
fn a_move_below_the_tolerance_is_still_reported() {
    let before = g1();
    let hole = first_of(&before, |e| matches!(e, Entity::Circle(_)));
    let mut after = before.clone();
    for e in entity_mut(&mut after, hole) {
        if let Entity::Circle(c) = e {
            c.center.x += 1e-7;
        }
    }
    let set = diff(&before, &after, DiffOptions::default());
    assert_eq!(kinds(&set), ["MODIFIED"]);
    assert_eq!(field_paths(&set.changes[0]), ["center.x"]);
    let Change::Modified(m) = &set.changes[0] else {
        unreachable!()
    };
    assert_eq!(m.fields[0].verdict, Verdict::Within);
}

/// A move of a hundredth of a unit: beyond the default tolerance, still
/// below the grid of a PDF export. `MODIFIED` with `BEYOND` when the states
/// share references; a `REMOVED` plus `ADDED` pair when they do not.
#[test]
fn a_move_below_any_visible_grid_is_beyond_tolerance() {
    let before = g1();
    let hole = first_of(&before, |e| matches!(e, Entity::Circle(_)));
    let mut after = before.clone();
    for e in entity_mut(&mut after, hole) {
        if let Entity::Circle(c) = e {
            c.center.x += 0.01;
        }
    }
    let set = diff(&before, &after, DiffOptions::default());
    assert_eq!(kinds(&set), ["MODIFIED"]);
    let Change::Modified(m) = &set.changes[0] else {
        unreachable!()
    };
    assert_eq!(m.fields[0].verdict, Verdict::Beyond);

    let set = diff(&before, &after, geometry());
    assert_eq!(kinds(&set), ["REMOVED", "ADDED"]);
}

/// Two coincident lines; one is deleted. The picture is the same line.
#[test]
fn one_of_two_coincident_entities_removed_is_reported() {
    let before = g6();
    let gone = before.entities[1].common().id;
    let mut after = before.clone();
    after.entities.remove(1);
    for block in after.tables.block_records.values_mut() {
        block.entities.retain(|e| e.common().id != gone);
    }

    let set = diff(&before, &after, DiffOptions::default());
    assert_eq!(kinds(&set), ["REMOVED"]);
    assert_eq!(set.changes[0].id(), gone);

    // Without shared references the diff cannot tell which one went, and
    // says so rather than guessing: both are UNKNOWN, neither is MODIFIED.
    let set = diff(&before, &after, geometry());
    assert_eq!(kinds(&set), ["UNKNOWN", "UNKNOWN"]);
}

/// The outline moves from layer `OUTLINE` to layer `0`, both colour 7, and
/// then its colour goes from BYLAYER to an explicit 7: the same ink, twice.
#[test]
fn a_layer_or_colour_change_that_keeps_the_same_ink_is_reported() {
    let before = g1();
    assert_eq!(before.tables.layers["OUTLINE"].color_index, 7);
    assert_eq!(before.tables.layers["0"].color_index, 7);
    let outline = first_of(&before, |e| matches!(e, Entity::LwPolyline(_)));
    let mut after = before.clone();
    for e in entity_mut(&mut after, outline) {
        if let Entity::LwPolyline(p) = e {
            p.common.layer = Ref::Resolved("0".to_string());
            p.common.color_index = 7;
        }
    }

    for options in [DiffOptions::default(), geometry()] {
        let set = diff(&before, &after, options);
        assert_eq!(kinds(&set), ["MODIFIED"], "{:?}", set.changes);
        assert_eq!(
            field_paths(&set.changes[0]),
            ["common.color_index", "common.layer.data"]
        );
    }
}

/// The title block's frame is drawn twice as large inside the block, and
/// the one instance is scaled by a half: every point lands where it was.
#[test]
fn a_block_edit_cancelled_by_its_instance_is_reported_on_both() {
    let before = g1();
    let mut after = before.clone();
    let block = after
        .tables
        .block_records
        .get_mut("TITLEBLOCK")
        .expect("the title block");
    let mut frame = None;
    for e in &mut block.entities {
        if let Entity::LwPolyline(p) = e {
            for v in &mut p.vertices {
                v.x *= 2.0;
                v.y *= 2.0;
            }
            frame = Some(p.common.id);
        }
    }
    let frame = frame.expect("the block has a frame");
    let insert = first_of(&before, |e| matches!(e, Entity::Insert(_)));
    for e in entity_mut(&mut after, insert) {
        if let Entity::Insert(i) = e {
            i.scale.x = 0.5;
            i.scale.y = 0.5;
            i.scale.z = 0.5;
        }
    }

    let set = diff(&before, &after, DiffOptions::default());
    assert_eq!(kinds(&set), ["MODIFIED", "MODIFIED"], "{:?}", set.changes);
    let ids: Vec<EntityId> = set.changes.iter().map(Change::id).collect();
    assert_eq!(ids, [frame, insert]);
    assert_eq!(
        field_paths(&set.changes[0]),
        [
            "vertices[1].x",
            "vertices[2].x",
            "vertices[2].y",
            "vertices[3].y"
        ]
    );
    assert_eq!(
        field_paths(&set.changes[1]),
        ["scale.x", "scale.y", "scale.z"]
    );
}
