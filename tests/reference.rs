//! The contract, checked on a real drawing's worth of entities: the first
//! golden case's expected model (`tests/golden/`) is the *before* state, and
//! the *after* states are made by editing it directly -- no editor crate is
//! involved, so what is tested is the diff alone.

use iron_diff_cad::{diff, Change, DiffOptions, Matching, Tolerance, Verdict};
use uncad_model::model::{Confidence, Entity, EntityId};
use uncad_model::CadDatabase;

fn g1() -> CadDatabase {
    serde_json::from_str(include_str!("golden/g1.expected.json"))
        .expect("the golden model deserializes")
}

/// The reference IDs of G1's four holes, in file order.
fn hole_ids(db: &CadDatabase) -> Vec<EntityId> {
    db.entities
        .iter()
        .filter_map(|e| match e {
            Entity::Circle(c) => Some(c.common.id),
            _ => None,
        })
        .collect()
}

/// Sets the radius of the circle with `id`, wherever it appears (the top
/// level and the block that owns it), so the edit is one entity's edit.
fn set_radius(db: &mut CadDatabase, id: EntityId, radius: f64) {
    let mut hit = 0;
    let blocks = db
        .tables
        .block_records
        .values_mut()
        .flat_map(|b| b.entities.iter_mut());
    for e in db.entities.iter_mut().chain(blocks) {
        if let Entity::Circle(c) = e {
            if c.common.id == id {
                c.radius = radius;
                hit += 1;
            }
        }
    }
    assert!(hit > 0, "circle {id:?} exists");
}

#[test]
fn a_single_edit_is_exactly_one_modified_entry_with_exactly_one_field() {
    let before = g1();
    let mut after = before.clone();
    let hole = hole_ids(&before)[0];
    set_radius(&mut after, hole, 6.0);

    let set = diff(&before, &after, DiffOptions::default());
    assert_eq!(set.matching, Matching::Reference);
    assert_eq!(set.changes.len(), 1, "{:?}", set.changes);
    let Change::Modified(m) = &set.changes[0] else {
        panic!("expected MODIFIED, got {:?}", set.changes[0]);
    };
    assert_eq!(m.id, hole);
    assert_eq!(m.entity_type, "CIRCLE");
    assert_eq!(m.confidence, Confidence::High);
    assert_eq!(m.fields.len(), 1);
    assert_eq!(m.fields[0].path, "radius");
    assert_eq!(m.fields[0].delta, Some(1.0));
    assert_eq!(m.fields[0].verdict, Verdict::Beyond);
}

#[test]
fn identical_states_have_an_empty_change_set() {
    let before = g1();
    let set = diff(&before, &before.clone(), DiffOptions::default());
    assert!(set.is_empty(), "{:?}", set.changes);
}

#[test]
fn the_tolerance_boundary_is_beyond() {
    let before = g1();
    let hole = hole_ids(&before)[1];
    // Binary fractions, so that `5.0 + tol` and the deltas are exact in
    // f64 and the boundary case really is the boundary.
    let tol = 0.25;
    let options = DiffOptions {
        tolerance: Tolerance {
            length: tol,
            angle: 1e-9,
        },
    };
    let eps = 0.0625;
    for (radius, expected) in [
        (5.0 + tol - eps, Verdict::Within),
        (5.0 + tol, Verdict::Beyond),
        (5.0 + tol + eps, Verdict::Beyond),
    ] {
        let mut after = before.clone();
        set_radius(&mut after, hole, radius);
        let set = diff(&before, &after, options);
        assert_eq!(
            set.tolerance.length, tol,
            "the tolerance is written into the result"
        );
        let Change::Modified(m) = &set.changes[0] else {
            panic!("expected MODIFIED for radius {radius}");
        };
        assert_eq!(
            m.fields.len(),
            1,
            "the field is listed whatever the verdict"
        );
        assert_eq!(m.fields[0].verdict, expected, "radius {radius}");
        assert_eq!(m.fields[0].tolerance, Some(tol));
    }
}

#[test]
fn an_edit_that_touches_more_than_it_was_asked_to_is_reported() {
    // The mutation check: a "wrong" edit that also moves a second hole must
    // never produce the change set of the right edit.
    let before = g1();
    let holes = hole_ids(&before);
    let mut right = before.clone();
    set_radius(&mut right, holes[0], 6.0);
    let mut wrong = right.clone();
    set_radius(&mut wrong, holes[2], 5.5);

    let right_set = diff(&before, &right, DiffOptions::default());
    let wrong_set = diff(&before, &wrong, DiffOptions::default());
    assert_ne!(right_set, wrong_set);
    assert_eq!(wrong_set.changes.len(), 2);
    assert!(
        wrong_set.changes.iter().any(|c| c.id() == holes[2]),
        "the unasked-for change is named"
    );
}

#[test]
fn added_and_removed_entities_are_reported_as_such() {
    let before = g1();
    let mut after = before.clone();
    let removed = after.entities.remove(1);
    let removed_id = removed.common().id;
    let mut added = removed.clone();
    if let Entity::Circle(c) = &mut added {
        c.common.id = EntityId::new(0x9999);
    }
    let added_id = added.common().id;
    after.entities.push(added);
    // The block record still lists the removed entity: remove it there too,
    // so the entity is gone from the drawing altogether.
    for b in after.tables.block_records.values_mut() {
        b.entities.retain(|e| e.common().id != removed_id);
    }

    let set = diff(&before, &after, DiffOptions::default());
    let kinds: Vec<(&str, EntityId)> = set
        .changes
        .iter()
        .map(|c| match c {
            Change::Added(e) => ("ADDED", e.id),
            Change::Removed(e) => ("REMOVED", e.id),
            Change::Modified(m) => ("MODIFIED", m.id),
            Change::Unknown(u) => ("UNKNOWN", u.id),
        })
        .collect();
    assert_eq!(kinds, [("REMOVED", removed_id), ("ADDED", added_id)]);
}

#[test]
fn a_change_carries_the_lower_confidence() {
    let before = g1();
    let mut after = before.clone();
    let hole = hole_ids(&before)[3];
    set_radius(&mut after, hole, 4.0);
    for e in after.entities.iter_mut() {
        if e.common().id == hole {
            if let Entity::Circle(c) = e {
                c.common.confidence = Confidence::Low;
            }
        }
    }
    let set = diff(&before, &after, DiffOptions::default());
    let Change::Modified(m) = &set.changes[0] else {
        panic!("expected MODIFIED");
    };
    assert_eq!(
        m.confidence,
        Confidence::Low,
        "never higher than the entities involved"
    );
    let paths: Vec<&str> = m.fields.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        ["common.confidence", "radius"],
        "the marker change is listed too"
    );
}

#[test]
fn the_inputs_are_not_modified() {
    let before = g1();
    let mut after = before.clone();
    set_radius(&mut after, hole_ids(&before)[0], 7.0);
    let before_copy = before.clone();
    let after_copy = after.clone();
    let _ = diff(&before, &after, DiffOptions::default());
    assert_eq!(before, before_copy);
    assert_eq!(after, after_copy);
}

#[test]
fn the_json_form_is_adjacently_tagged() {
    let before = g1();
    let mut after = before.clone();
    set_radius(&mut after, hole_ids(&before)[0], 6.0);
    let json = diff(&before, &after, DiffOptions::default())
        .to_json(false)
        .unwrap();
    assert!(json.starts_with("{\"matching\":\"REFERENCE\",\"tolerance\":{"));
    assert!(json.contains("{\"type\":\"MODIFIED\",\"data\":{"));
    assert!(json.contains("\"verdict\":\"BEYOND\""));
    assert!(json.contains("\"provenance\":[\"VECTOR\",\"VECTOR\"]"));
    let back: iron_diff_cad::ChangeSet = serde_json::from_str(&json).unwrap();
    assert_eq!(back.changes.len(), 1);
}
