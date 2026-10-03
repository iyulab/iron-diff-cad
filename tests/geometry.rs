//! Geometric matching, checked on the golden cases: the *after* state is
//! the same drawing with every reference ID reissued, so the two states
//! share no references and only type and shape can pair their entities.

use iron_diff_cad::{diff, Change, DiffOptions, Matching, Pairing, Verdict};
use serde_json::Value;
use uncad_model::model::{Entity, EntityId, Point3D, Ref};
use uncad_model::CadDatabase;

/// Pairing by reference ID, as a caller that knows the two states are one
/// drawing asks for it: these states are an edit of one golden drawing,
/// whose file states no fingerprint.
fn by_reference() -> DiffOptions {
    DiffOptions {
        matching: iron_diff_cad::Matching::Reference,
        ..DiffOptions::default()
    }
}

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
/// was issued from) reissued -- nested entities too, such as an INSERT's
/// attributes: what a second revision looks like to a diff.
fn reissued(db: &CadDatabase) -> CadDatabase {
    let mut value = serde_json::to_value(db).expect("the model serializes");
    fn reissue(v: &mut Value) {
        match v {
            Value::Object(fields) => {
                if let Some(Value::Object(common)) = fields.get_mut("common") {
                    let id = common["id"].as_u64().expect("an id") + REISSUE;
                    common["id"] = Value::from(id);
                    common["source_handle"] =
                        serde_json::json!({"type": "RESOLVED", "data": format!("{id:X}")});
                }
                for (key, field) in fields.iter_mut() {
                    if key != "common" {
                        reissue(field);
                    }
                }
            }
            Value::Array(items) => items.iter_mut().for_each(reissue),
            _ => {}
        }
    }
    fn reissue_entities(entities: &mut Value) {
        assert!(entities.is_array(), "an entity list");
        reissue(entities);
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
fn a_moved_entity_is_paired_by_similarity_with_the_reason_stated() {
    let before = g1();
    let mut after = reissued(&before);
    let hole = hole_ids(&before)[0];
    move_circle(&mut after, after_id(hole), 10.0);

    let set = diff(&before, &after, geometry());
    assert_eq!(set.pairing, Some(Pairing::default()));
    assert_eq!(set.changes.len(), 1, "{:?}", set.changes);
    let Change::Modified(m) = &set.changes[0] else {
        panic!("expected MODIFIED, got {:?}", set.changes[0]);
    };
    assert_eq!(m.id, hole);
    assert_eq!(m.counterpart, Some(after_id(hole)));
    let paths: Vec<&str> = m.fields.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["center.x"]);
    // Every other hole was paired by its unchanged shape, so the moved one
    // has no rival: six of its seven leaves agree.
    let matched_by = m.matched_by.expect("paired by similarity");
    assert_eq!(matched_by.similarity, 6.0 / 7.0);
    assert_eq!(matched_by.runner_up, None);
}

#[test]
fn with_no_similarity_enough_a_moved_entity_is_removed_plus_added() {
    let before = g1();
    let mut after = reissued(&before);
    let hole = hole_ids(&before)[0];
    move_circle(&mut after, after_id(hole), 10.0);

    let set = diff(&before, &after, exact_shapes_only());
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

/// Geometric matching that pairs nothing but agreeing shapes: no
/// similarity reaches a threshold above 1.
fn exact_shapes_only() -> DiffOptions {
    DiffOptions {
        pairing: Pairing {
            min_similarity: 1.5,
            ..Pairing::default()
        },
        ..geometry()
    }
}

fn set_radius(db: &mut CadDatabase, id: EntityId, radius: f64) {
    for e in circles(db, id) {
        if let Entity::Circle(c) = e {
            c.radius = radius;
        }
    }
}

fn set_measurement(db: &mut CadDatabase, id: EntityId, measurement: f64) {
    let blocks = db
        .tables
        .block_records
        .values_mut()
        .flat_map(|b| b.entities.iter_mut());
    let mut hit = 0;
    for e in db.entities.iter_mut().chain(blocks) {
        if let Entity::Dimension(d) = e {
            if d.common.id == id {
                d.measurement = Some(measurement);
                hit += 1;
            }
        }
    }
    assert!(hit > 0, "dimension {id:?} exists");
}

/// G1's diameter dimension, on its first hole.
fn hole_diameter() -> EntityId {
    EntityId::new(296)
}

/// A field change as `(path, before, after)`.
type FieldValues = (String, Value, Value);

/// Each `MODIFIED` entry's entity and its field changes, by entity.
fn modified_fields(set: &iron_diff_cad::ChangeSet) -> Vec<(EntityId, Vec<FieldValues>)> {
    let mut out: Vec<(EntityId, Vec<FieldValues>)> = set
        .changes
        .iter()
        .map(|c| match c {
            Change::Modified(m) => (
                m.id,
                m.fields
                    .iter()
                    .map(|f| (f.path.clone(), f.before.clone(), f.after.clone()))
                    .collect(),
            ),
            other => panic!("expected MODIFIED, got {other:?}"),
        })
        .collect();
    out.sort_by_key(|(id, _)| *id);
    out
}

#[test]
fn a_resized_hole_and_its_remeasured_dimension_read_as_by_reference() {
    // A revision that resizes a hole and records the new diameter: matched
    // by geometry, the same entries -- same fields, same values -- as a
    // comparison of the two states by reference.
    let mut before = g1();
    set_measurement(&mut before, hole_diameter(), 10.0);
    let hole = hole_ids(&before)[0];
    let mut edited = before.clone();
    set_radius(&mut edited, hole, 6.0);
    set_measurement(&mut edited, hole_diameter(), 12.0);

    let expected = modified_fields(&diff(&before, &edited, by_reference()));
    assert_eq!(
        expected,
        [
            (hole, vec![("radius".into(), 5.0.into(), 6.0.into())]),
            (
                hole_diameter(),
                vec![("measurement".into(), 10.0.into(), 12.0.into())]
            ),
        ]
    );
    let got = modified_fields(&diff(&before, &reissued(&edited), geometry()));
    assert_eq!(got, expected);
}

#[test]
fn two_holes_changed_alike_pair_when_each_singles_the_other_out() {
    // The two lower holes, (20, 20) and (180, 20), both resized 5 -> 6:
    // each new hole agrees with its old one on everything but the radius,
    // and with the other old one on less (the x differs too).
    let before = g1();
    let holes = hole_ids(&before);
    let mut after = reissued(&before);
    for &h in &holes[..2] {
        set_radius(&mut after, after_id(h), 6.0);
    }
    let set = diff(&before, &after, geometry());
    assert_eq!(set.changes.len(), 2, "{:?}", set.changes);
    for (change, &h) in set.changes.iter().zip(&holes[..2]) {
        let Change::Modified(m) = change else {
            panic!("expected MODIFIED, got {change:?}");
        };
        assert_eq!((m.id, m.counterpart), (h, Some(after_id(h))));
        let matched_by = m.matched_by.expect("paired by similarity");
        assert_eq!(matched_by.similarity, 6.0 / 7.0);
        assert_eq!(matched_by.runner_up, Some(5.0 / 7.0));
    }
}

#[test]
fn two_holes_equally_similar_to_two_others_are_unknown_never_a_pick() {
    // Two holes at (0, 0) and (50, 50) become two at (0, 50) and (50, 0),
    // resized: each old hole shares one coordinate with each new one.
    // Nothing tells the pairs apart, so neither is chosen.
    let mut before = g1();
    let holes = hole_ids(&before);
    let place = |db: &mut CadDatabase, id: EntityId, x: f64, y: f64, r: f64| {
        set_center(db, id, Point3D { x, y, z: 0.0 });
        set_radius(db, id, r);
    };
    place(&mut before, holes[0], 0.0, 0.0, 5.0);
    place(&mut before, holes[1], 50.0, 50.0, 5.0);
    let mut after = reissued(&before);
    place(&mut after, after_id(holes[0]), 0.0, 50.0, 6.0);
    place(&mut after, after_id(holes[1]), 50.0, 0.0, 6.0);

    let set = diff(&before, &after, geometry());
    assert_eq!(set.changes.len(), 2, "{:?}", set.changes);
    for change in &set.changes {
        let Change::Unknown(u) = change else {
            panic!("expected UNKNOWN, got {change:?}");
        };
        assert_eq!(u.candidates, [after_id(holes[0]), after_id(holes[1])]);
        assert_eq!(u.similarities, Some(vec![5.0 / 7.0, 5.0 / 7.0]));
        assert!(
            u.reason
                .starts_with("2 entities of the second state are equally similar"),
            "{}",
            u.reason
        );
    }
}

#[test]
fn a_pair_too_close_to_its_runner_up_is_unknown() {
    // The two lower holes resized as before, under a margin the scores
    // cannot clear (6/7 against 5/7): each is UNKNOWN with both candidates.
    let before = g1();
    let holes = hole_ids(&before);
    let mut after = reissued(&before);
    for &h in &holes[..2] {
        set_radius(&mut after, after_id(h), 6.0);
    }
    let options = DiffOptions {
        pairing: Pairing {
            min_margin: 0.2,
            ..Pairing::default()
        },
        ..geometry()
    };
    let set = diff(&before, &after, options);
    assert_eq!(set.pairing.map(|p| p.min_margin), Some(0.2));
    assert_eq!(set.changes.len(), 2, "{:?}", set.changes);
    for change in &set.changes {
        let Change::Unknown(u) = change else {
            panic!("expected UNKNOWN, got {change:?}");
        };
        assert_eq!(u.candidates, [after_id(holes[0]), after_id(holes[1])]);
        assert!(
            u.reason.contains("stands less than 0.2 above the next"),
            "{}",
            u.reason
        );
    }
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
    let set = diff(&before, &after, by_reference());
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

    let set = diff(&before, &after, exact_shapes_only());
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
fn a_polyline_whose_only_change_is_a_bulge_is_paired_with_that_field() {
    // G1's outline, in the second revision with every ID reissued -- so it
    // can only be paired by its shape -- with one straight edge turned into
    // an arc. A bulge is part of the shape, so the shapes differ; the
    // outline is still the one polyline nearly all of whose leaves agree,
    // and the change is that one field.
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
    assert_eq!(set.changes.len(), 1, "{:?}", set.changes);
    let Change::Modified(m) = &set.changes[0] else {
        panic!("expected MODIFIED, got {:?}", set.changes);
    };
    assert_eq!(m.entity_type, "LWPOLYLINE");
    assert_eq!(m.counterpart, Some(outline_after));
    let paths: Vec<&str> = m.fields.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["vertices[1].bulge"]);

    // With only agreeing shapes paired, it is removed and added.
    let set = diff(&before, &after, exact_shapes_only());
    assert_eq!(set.changes.len(), 2, "{:?}", set.changes);
    let (Change::Removed(removed), Change::Added(added)) = (&set.changes[0], &set.changes[1])
    else {
        panic!("expected REMOVED then ADDED, got {:?}", set.changes);
    };
    assert_eq!(removed.entity_type, "LWPOLYLINE");
    assert_eq!(added.id, outline_after);
    assert_eq!(removed.id.value() + REISSUE, outline_after.value());
}

/// The same drawing saved again: a save renumbers the anonymous blocks that
/// draw the dimensions (`*D1` becomes `*D11`, ...), block records included,
/// while every dimension stays as it was.
fn dimension_blocks_renumbered(db: &CadDatabase) -> CadDatabase {
    fn renamed(name: &str) -> String {
        match name.strip_prefix("*D") {
            Some(n) => format!("*D{}", n.parse::<u32>().expect("a numbered block") + 10),
            None => name.to_string(),
        }
    }
    /// Renames the block of every dimension in `entities`; how many there were.
    fn rename(entities: &mut serde_json::Value) -> usize {
        let mut n = 0;
        for e in entities.as_array_mut().expect("an entity list") {
            if e["type"] == "DIMENSION" {
                let name = renamed(e["block_name"]["data"].as_str().expect("a named block"));
                e["block_name"]["data"] = serde_json::Value::from(name);
                n += 1;
            }
        }
        n
    }
    let mut value = serde_json::to_value(db).expect("the model serializes");
    let mut seen = rename(&mut value["entities"]);
    let records = value["tables"]["block_records"]
        .as_object_mut()
        .expect("block records");
    for (name, mut record) in std::mem::take(records) {
        seen += rename(&mut record["entities"]);
        let name = renamed(&name);
        record["name"] = serde_json::Value::from(name.clone());
        records.insert(name, record);
    }
    assert!(seen > 0, "the drawing has dimensions");
    serde_json::from_value(value).expect("the renumbered model deserializes")
}

#[test]
fn a_dimension_whose_anonymous_block_a_save_renumbered_keeps_its_counterpart() {
    let before = g1();
    let after = reissued(&dimension_blocks_renumbered(&before));
    let set = diff(&before, &after, geometry());
    assert!(set.is_empty(), "{:?}", set.changes);
}
