//! Geometric matching, checked on the golden cases: the *after* state is
//! the same drawing with every reference ID reissued, so the two states
//! share no references and only type and shape can pair their entities.

use iron_diff_cad::{diff, Change, DiffOptions, Matching, Verdict};
use serde_json::Value;
use uncad_model::model::{Entity, EntityId, Point3D, Ref};
use uncad_model::CadDatabase;

fn g1() -> CadDatabase {
    serde_json::from_str(include_str!("golden/g1.expected.json"))
        .expect("the golden model deserializes")
}

fn g6() -> CadDatabase {
    serde_json::from_str(include_str!("golden/g6.expected.json"))
        .expect("the golden model deserializes")
}

/// Every reference ID moved up by this much in the second revision, so
/// that no ID is shared and none collides.
const REISSUE: u64 = 0x1000;

/// The same drawing with every entity's reference ID (and the handle it
/// was issued from) reissued: what a second revision looks like to a diff.
fn reissued(db: &CadDatabase) -> CadDatabase {
    let mut value = serde_json::to_value(db).expect("the model serializes");
    fn reissue_entities(entities: &mut Value) {
        for e in entities.as_array_mut().expect("an entity list") {
            let common = e["common"].as_object_mut().expect("common");
            let id = common["id"].as_u64().expect("an id") + REISSUE;
            common["id"] = Value::from(id);
            common["source_handle"] =
                serde_json::json!({"type": "RESOLVED", "data": format!("{id:X}")});
        }
    }
    reissue_entities(&mut value["entities"]);
    for block in value["tables"]["block_records"]
        .as_object_mut()
        .expect("block records")
        .values_mut()
    {
        reissue_entities(&mut block["entities"]);
    }
    serde_json::from_value(value).expect("the reissued model deserializes")
}

fn geometry() -> DiffOptions {
    DiffOptions {
        matching: Matching::Geometry,
        ..DiffOptions::default()
    }
}

/// The circle with `id`, wherever it appears.
fn circles(db: &mut CadDatabase, id: EntityId) -> impl Iterator<Item = &mut Entity> {
    let blocks = db
        .tables
        .block_records
        .values_mut()
        .flat_map(|b| b.entities.iter_mut());
    db.entities
        .iter_mut()
        .chain(blocks)
        .filter(move |e| e.common().id == id)
}

fn hole_ids(db: &CadDatabase) -> Vec<EntityId> {
    db.entities
        .iter()
        .filter_map(|e| match e {
            Entity::Circle(c) => Some(c.common.id),
            _ => None,
        })
        .collect()
}

fn move_circle(db: &mut CadDatabase, id: EntityId, dx: f64) {
    let mut hit = 0;
    for e in circles(db, id) {
        if let Entity::Circle(c) = e {
            c.center.x += dx;
            hit += 1;
        }
    }
    assert!(hit > 0, "circle {id:?} exists");
}

fn set_center(db: &mut CadDatabase, id: EntityId, center: Point3D) {
    for e in circles(db, id) {
        if let Entity::Circle(c) = e {
            c.center = center;
        }
    }
}

fn after_id(before: EntityId) -> EntityId {
    EntityId::new(before.value() + REISSUE)
}

#[test]
fn identical_revisions_with_no_shared_references_have_an_empty_change_set() {
    let before = g1();
    let after = reissued(&before);
    assert_ne!(
        before.entities[0].common().id,
        after.entities[0].common().id,
        "the revisions share no references"
    );
    let set = diff(&before, &after, geometry());
    assert_eq!(set.matching, Matching::Geometry);
    assert!(set.is_empty(), "{:?}", set.changes);
}

#[test]
fn a_moved_entity_is_removed_plus_added_never_a_guess() {
    let before = g1();
    let mut after = reissued(&before);
    let hole = hole_ids(&before)[0];
    move_circle(&mut after, after_id(hole), 10.0);

    let set = diff(&before, &after, geometry());
    assert_eq!(set.changes.len(), 2, "{:?}", set.changes);
    let Change::Removed(removed) = &set.changes[0] else {
        panic!("expected REMOVED first, got {:?}", set.changes[0]);
    };
    let Change::Added(added) = &set.changes[1] else {
        panic!("expected ADDED second, got {:?}", set.changes[1]);
    };
    assert_eq!(removed.id, hole);
    assert_eq!(added.id, after_id(hole));
    assert_eq!(removed.entity_type, "CIRCLE");
}

#[test]
fn two_identical_entities_are_unknown_with_both_candidates() {
    let mut before = g1();
    let holes = hole_ids(&before);
    // Make the second hole identical to the first: same center, same
    // radius, same layer.
    let first_center = match &before.entities[1] {
        Entity::Circle(c) => c.center,
        other => panic!("{other:?}"),
    };
    set_center(&mut before, holes[1], first_center);
    let after = reissued(&before);

    let set = diff(&before, &after, geometry());
    assert_eq!(set.changes.len(), 2, "{:?}", set.changes);
    let candidates = vec![after_id(holes[0]), after_id(holes[1])];
    for (change, id) in set.changes.iter().zip([holes[0], holes[1]]) {
        let Change::Unknown(u) = change else {
            panic!("expected UNKNOWN, got {change:?}");
        };
        assert_eq!(u.id, id);
        assert_eq!(u.candidates, candidates);
        assert!(
            u.reason
                .starts_with("2 entities of the second state match this CIRCLE"),
            "{}",
            u.reason
        );
    }
}

#[test]
fn one_of_two_overlapping_entities_removed_is_unknown_for_both() {
    // G6: two identical lines. The second revision keeps one of them; the
    // diff cannot know which, and says so for both.
    let before = g6();
    let mut after = reissued(&before);
    let kept = after.entities[0].common().id;
    after.entities.remove(1);
    for block in after.tables.block_records.values_mut() {
        block.entities.retain(|e| e.common().id == kept);
    }

    let set = diff(&before, &after, geometry());
    assert_eq!(set.changes.len(), 2, "{:?}", set.changes);
    for change in &set.changes {
        let Change::Unknown(u) = change else {
            panic!("expected UNKNOWN, got {change:?}");
        };
        assert_eq!(u.candidates, vec![kept]);
        assert!(
            u.reason.contains("also matches 1 other entity"),
            "{}",
            u.reason
        );
    }
}

#[test]
fn a_certain_match_lists_its_counterpart_and_only_the_fields_that_differ() {
    let before = g1();
    let mut after = reissued(&before);
    let hole = hole_ids(&before)[2];
    for e in circles(&mut after, after_id(hole)) {
        if let Entity::Circle(c) = e {
            c.common.layer = Ref::Resolved("OUTLINE".to_string());
        }
    }

    let set = diff(&before, &after, geometry());
    assert_eq!(set.changes.len(), 1, "{:?}", set.changes);
    let Change::Modified(m) = &set.changes[0] else {
        panic!("expected MODIFIED, got {:?}", set.changes[0]);
    };
    assert_eq!(m.id, hole);
    assert_eq!(m.counterpart, Some(after_id(hole)));
    let paths: Vec<&str> = m.fields.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        ["common.layer.data"],
        "the reissued handle is not a change"
    );
    assert_eq!(m.fields[0].verdict, Verdict::Beyond);
    let json = set.to_json(false).unwrap();
    assert!(json.contains("\"counterpart\":"), "{json}");
}

#[test]
fn under_reference_matching_the_counterpart_is_absent() {
    let before = g1();
    let mut after = before.clone();
    let hole = hole_ids(&before)[0];
    move_circle(&mut after, hole, 1.0);
    let set = diff(&before, &after, DiffOptions::default());
    let Change::Modified(m) = &set.changes[0] else {
        panic!("{:?}", set.changes[0]);
    };
    assert_eq!(m.counterpart, None);
    assert!(!set.to_json(false).unwrap().contains("counterpart"));
}

#[test]
fn entries_are_ordered_by_type_then_representative_point() {
    let before = g1();
    let mut after = reissued(&before);
    let holes = hole_ids(&before);
    // Two holes move: (20,20) -> (30,20) and (180,80) -> (190,80).
    move_circle(&mut after, after_id(holes[0]), 10.0);
    move_circle(&mut after, after_id(holes[3]), 10.0);

    let set = diff(&before, &after, geometry());
    let summary: Vec<(&str, EntityId)> = set
        .changes
        .iter()
        .map(|c| {
            (
                match c {
                    Change::Added(_) => "ADDED",
                    Change::Removed(_) => "REMOVED",
                    Change::Modified(_) => "MODIFIED",
                    Change::Unknown(_) => "UNKNOWN",
                },
                c.id(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("REMOVED", holes[0]),
            ("ADDED", after_id(holes[0])),
            ("REMOVED", holes[3]),
            ("ADDED", after_id(holes[3])),
        ],
        "by x of the representative point, whichever state the entry is from"
    );
}

#[test]
fn the_same_two_revisions_give_the_same_bytes() {
    let mut before = g1();
    let holes = hole_ids(&before);
    let center = match &before.entities[1] {
        Entity::Circle(c) => c.center,
        other => panic!("{other:?}"),
    };
    set_center(&mut before, holes[1], center);
    let mut after = reissued(&before);
    move_circle(&mut after, after_id(holes[2]), 3.0);
    let first = diff(&before, &after, geometry()).to_json(false).unwrap();
    for _ in 0..24 {
        assert_eq!(
            diff(&before, &after, geometry()).to_json(false).unwrap(),
            first
        );
    }
}

#[test]
fn a_polyline_whose_only_change_is_a_bulge_has_changed_shape() {
    // G1's outline, in the second revision with every ID reissued -- so it
    // can only be paired by its shape -- with one straight edge turned into
    // an arc. A bulge is part of the shape: the outline is removed and
    // added, like any other entity whose geometry changed, and nothing else
    // is reported.
    let before = g1();
    let mut after = reissued(&before);
    let outline = after
        .entities
        .iter_mut()
        .find_map(|e| match e {
            Entity::LwPolyline(p) => Some(p),
            _ => None,
        })
        .expect("G1 has an outline");
    outline.vertices[1].bulge = 0.25;
    let outline_after = outline.common.id;

    let set = diff(&before, &after, geometry());
    assert_eq!(set.changes.len(), 2, "{:?}", set.changes);
    let (Change::Removed(removed), Change::Added(added)) = (&set.changes[0], &set.changes[1])
    else {
        panic!("expected REMOVED then ADDED, got {:?}", set.changes);
    };
    assert_eq!(removed.entity_type, "LWPOLYLINE");
    assert_eq!(added.id, outline_after);
    assert_eq!(removed.id.value() + REISSUE, outline_after.value());
}
