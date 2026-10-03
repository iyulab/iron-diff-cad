//! The lineage verdict, and the mode it picks under `AUTO`: the golden
//! cases, with the header GUIDs each pair of files would state.

use iron_diff_cad::{diff, lineage, Change, DiffOptions, LineageVerdict, Matching};
use uncad_model::model::Entity;
use uncad_model::CadDatabase;

fn golden(json: &str) -> CadDatabase {
    serde_json::from_str(json).expect("the golden model deserializes")
}

fn g1() -> CadDatabase {
    golden(include_str!("golden/g1.expected.json"))
}

fn g5() -> CadDatabase {
    golden(include_str!("golden/g5.expected.json"))
}

/// `db` as a file stating these GUIDs would read.
fn stating(mut db: CadDatabase, fingerprint: &str, version: &str) -> CadDatabase {
    db.header.fingerprintguid = Some(fingerprint.to_string());
    db.header.versionguid = Some(version.to_string());
    db
}

/// `db` with the first circle's radius changed, wherever it is listed.
fn one_radius_changed(db: &CadDatabase) -> CadDatabase {
    let mut after = db.clone();
    let id = after
        .entities
        .iter()
        .find_map(|e| match e {
            Entity::Circle(c) => Some(c.common.id),
            _ => None,
        })
        .expect("a circle");
    for e in after.all_entities_mut() {
        if let Entity::Circle(c) = e {
            if c.common.id == id {
                c.radius += 1.0;
            }
        }
    }
    after
}

const A: &str = "{AAAAAAAA-0000-0000-0000-000000000001}";
const B: &str = "{BBBBBBBB-0000-0000-0000-000000000002}";

#[test]
fn one_drawing_saved_after_an_edit_is_the_same_lineage_and_auto_pairs_by_reference() {
    let before = stating(g1(), A, "{V1}");
    let after = stating(one_radius_changed(&before), A, "{V2}");
    let set = diff(&before, &after, DiffOptions::default());
    let l = set.lineage.as_ref().expect("a lineage");
    assert_eq!(l.verdict, LineageVerdict::Same);
    assert_eq!(
        (l.fingerprint_equal, l.version_equal),
        (Some(true), Some(false))
    );
    assert_eq!((l.shared, l.cross_type), (l.smaller, 0));
    assert_eq!(set.matching, Matching::Reference);
    assert_eq!(set.changes.len(), 1, "{:?}", set.changes);
    assert!(matches!(set.changes[0], Change::Modified(_)));
}

#[test]
fn a_different_fingerprint_is_another_lineage_and_auto_does_not_pair_by_reference() {
    let before = stating(g1(), A, "{V1}");
    let after = stating(g1(), B, "{V1}");
    let set = diff(&before, &after, DiffOptions::default());
    let l = set.lineage.as_ref().expect("a lineage");
    assert_eq!(l.verdict, LineageVerdict::Different);
    assert_eq!(l.fingerprint_equal, Some(false));
    assert_eq!(set.matching, Matching::Geometry);
    // The same entities, matched by shape: nothing changed.
    assert!(set.is_empty(), "{:?}", set.changes);
}

#[test]
fn an_id_holding_two_types_is_another_lineage_even_under_one_fingerprint() {
    // Two unrelated golden drawings whose IDs overlap, as if made from one
    // template.
    let set = diff(
        &stating(g1(), A, "{V1}"),
        &stating(g5(), A, "{V2}"),
        DiffOptions::default(),
    );
    let l = set.lineage.as_ref().expect("a lineage");
    assert!(l.cross_type > 0, "{l:?}");
    assert_eq!(l.verdict, LineageVerdict::Different);
    assert_eq!(set.matching, Matching::Geometry);
}

#[test]
fn without_a_stated_fingerprint_the_lineage_is_unknown_and_auto_takes_geometry() {
    let before = g1();
    let after = one_radius_changed(&before);
    let set = diff(&before, &after, DiffOptions::default());
    let l = set.lineage.as_ref().expect("a lineage");
    assert_eq!(l.verdict, LineageVerdict::Unknown);
    assert_eq!((l.fingerprint_equal, l.version_equal), (None, None));
    assert_eq!(set.matching, Matching::Geometry);
}

#[test]
fn too_few_shared_ids_for_the_threshold_is_unknown_and_the_threshold_is_stated() {
    let before = stating(g1(), A, "{V1}");
    let after = stating(g1(), A, "{V1}");
    // Every ID is shared: the share is 1, short of a threshold above it.
    let l = lineage(&before, &after, 1.5);
    assert_eq!(l.shared, l.smaller);
    assert_eq!(l.verdict, LineageVerdict::Unknown);
    assert_eq!(l.threshold, 1.5);
    let options = DiffOptions {
        shared_threshold: 1.5,
        ..DiffOptions::default()
    };
    assert_eq!(diff(&before, &after, options).matching, Matching::Geometry);
}

#[test]
fn an_entity_the_reader_could_not_decode_is_not_a_type_conflict() {
    let before = stating(g1(), A, "{V1}");
    let mut after = before.clone();
    let circle = after
        .entities
        .iter()
        .find(|e| matches!(e, Entity::Circle(_)))
        .expect("a circle")
        .common()
        .clone();
    for e in after.all_entities_mut() {
        if e.common().id == circle.id {
            *e = Entity::Unknown {
                common: circle.clone(),
                type_name: "ACAD_TABLE".to_string(),
            };
        }
    }
    let l = lineage(&before, &after, iron_diff_cad::DEFAULT_SHARED_THRESHOLD);
    assert_eq!(l.cross_type, 0);
    assert_eq!(l.verdict, LineageVerdict::Same);
}

#[test]
fn a_mode_the_caller_chose_is_kept_and_the_lineage_still_reported() {
    let options = DiffOptions {
        matching: Matching::Reference,
        ..DiffOptions::default()
    };
    let set = diff(
        &stating(g1(), A, "{V1}"),
        &stating(g1(), B, "{V1}"),
        options,
    );
    assert_eq!(set.matching, Matching::Reference);
    let json = set.to_json(false).expect("serializes");
    assert!(
        json.contains(r#""lineage":{"verdict":"DIFFERENT","fingerprint_equal":false"#),
        "{json}"
    );
}
