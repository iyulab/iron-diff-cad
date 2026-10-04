//! A reference whose name is the save's is compared by what it points at:
//! an INSERT's anonymous block by the entities it holds, an IMAGE's
//! definition (named by handle) by the file and size it states.

use iron_diff_cad::{diff, Change, ChangeSet, DiffOptions, Matching};
use serde_json::{json, Value};
use uncad_model::CadDatabase;

/// G1's title-block INSERT (listed in the drawing and under model space).
const TITLE_INSERT: u64 = 297;

fn g1() -> Value {
    serde_json::from_str(include_str!("golden/g1.expected.json"))
        .expect("the golden model deserializes")
}

fn options(matching: Matching) -> DiffOptions {
    DiffOptions {
        matching,
        ..DiffOptions::default()
    }
}

/// Moves every reference ID in `v` -- an entity's and its attributes' -- up
/// by `by`, so a copy collides with nothing.
fn reissue(v: &mut Value, by: u64) {
    match v {
        Value::Object(m) => {
            if m.contains_key("origin") {
                if let Some(id) = m.get("id").and_then(Value::as_u64) {
                    m.insert("id".into(), Value::from(id + by));
                }
            }
            m.values_mut().for_each(|c| reissue(c, by));
        }
        Value::Array(a) => a.iter_mut().for_each(|c| reissue(c, by)),
        _ => {}
    }
}

fn retarget(list: &mut Value, name: &str) {
    for e in list.as_array_mut().unwrap() {
        if e["common"]["id"] == TITLE_INSERT {
            e["block_name"]["data"] = Value::from(name);
        }
    }
}

/// Points every listing of the title-block INSERT in `db` at `name`.
fn point_title_at(db: &mut Value, name: &str) {
    retarget(&mut db["entities"], name);
    for b in db["tables"]["block_records"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        retarget(&mut b["entities"], name);
    }
}

/// Adds block `name` to `db`: a copy of the title block, its entities'
/// IDs moved up by `id_shift`, its outline's first vertex moved by `dx`.
fn add_title_copy(db: &mut Value, name: &str, id_shift: u64, dx: f64) {
    let mut block = db["tables"]["block_records"]["TITLEBLOCK"].clone();
    block["name"] = Value::from(name);
    reissue(&mut block["entities"], id_shift);
    let x = &mut block["entities"][0]["vertices"][0]["point"]["x"];
    *x = Value::from(x.as_f64().unwrap() + dx);
    db["tables"]["block_records"][name] = block;
}

/// Adds block `outer` to `db`, holding one INSERT (a copy of the title
/// INSERT, IDs moved up by `id_shift`) of block `inner`.
fn add_wrapper(db: &mut Value, outer: &str, inner: &str, id_shift: u64) {
    let mut insert = db["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["common"]["id"] == TITLE_INSERT)
        .unwrap()
        .clone();
    reissue(&mut insert, id_shift);
    insert["block_name"]["data"] = Value::from(inner);
    let mut block = db["tables"]["block_records"]["TITLEBLOCK"].clone();
    block["name"] = Value::from(outer);
    block["entities"] = Value::Array(vec![insert]);
    db["tables"]["block_records"][outer] = block;
}

fn model(v: Value) -> CadDatabase {
    serde_json::from_value(v).expect("the edited model deserializes")
}

/// The title INSERT's changed field paths, `None` when it is not modified.
fn title_fields(set: &ChangeSet) -> Option<Vec<String>> {
    set.changes.iter().find_map(|c| match c {
        Change::Modified(m) if m.id.value() == TITLE_INSERT => {
            Some(m.fields.iter().map(|f| f.path.clone()).collect())
        }
        _ => None,
    })
}

/// The same drawing saved again: the title INSERT points at an anonymous
/// block whose number changed and whose entities did not.
fn renumbered(dx: f64) -> (CadDatabase, CadDatabase) {
    let (mut before, mut after) = (g1(), g1());
    add_title_copy(&mut before, "*U24", 0x5000, 0.0);
    point_title_at(&mut before, "*U24");
    add_title_copy(&mut after, "*U96", 0x5000, dx);
    point_title_at(&mut after, "*U96");
    (model(before), model(after))
}

#[test]
fn a_renumbered_anonymous_block_that_holds_the_same_is_no_change() {
    let (before, after) = renumbered(0.0);
    let set = diff(&before, &after, options(Matching::Reference));
    assert!(set.changes.is_empty(), "{:?}", set.changes.first());

    // Geometric matching pairs the INSERT by its shape, which leaves the
    // anonymous name out, and then finds nothing changed. (The copied
    // block's entities equal the title block's, so they are UNKNOWN here:
    // two candidates each -- that is geometric matching, not the INSERT.)
    let set = diff(&before, &after, options(Matching::Geometry));
    let title: Vec<&Change> = set
        .changes
        .iter()
        .filter(|c| c.id().value() == TITLE_INSERT)
        .collect();
    assert!(title.is_empty(), "{title:?}");
}

#[test]
fn an_anonymous_block_that_holds_something_else_keeps_both_names() {
    let (before, after) = renumbered(1.0);
    let set = diff(&before, &after, options(Matching::Reference));
    assert_eq!(
        title_fields(&set),
        Some(vec!["block_name.data".to_string()])
    );
    let change = set
        .changes
        .iter()
        .find_map(|c| match c {
            Change::Modified(m) if m.id.value() == TITLE_INSERT => Some(m),
            _ => None,
        })
        .unwrap();
    assert_eq!(change.fields[0].before, "*U24");
    assert_eq!(change.fields[0].after, "*U96");
}

#[test]
fn a_difference_within_tolerance_is_the_same_block() {
    let (before, after) = renumbered(1e-9);
    let set = diff(&before, &after, options(Matching::Reference));
    assert_eq!(title_fields(&set), None);
}

#[test]
fn a_named_block_is_the_drawings_own_word() {
    let before = model(g1());
    let mut after = g1();
    add_title_copy(&mut after, "*U96", 0x5000, 0.0);
    point_title_at(&mut after, "*U96");
    let set = diff(&before, &model(after), options(Matching::Reference));
    assert_eq!(
        title_fields(&set),
        Some(vec!["block_name.data".to_string()])
    );
}

#[test]
fn nested_anonymous_blocks_are_compared_all_the_way_down() {
    let nested = |inner_dx: f64, outer: &str, inner: &str| {
        let mut v = g1();
        add_title_copy(&mut v, inner, 0x5000, inner_dx);
        add_wrapper(&mut v, outer, inner, 0x6000);
        point_title_at(&mut v, outer);
        model(v)
    };
    let before = nested(0.0, "*U2", "*U3");
    let same = diff(
        &before,
        &nested(0.0, "*U8", "*U9"),
        options(Matching::Reference),
    );
    assert!(same.changes.is_empty(), "{:?}", same.changes.first());

    let moved = diff(
        &before,
        &nested(1.0, "*U8", "*U9"),
        options(Matching::Reference),
    );
    assert_eq!(
        title_fields(&moved),
        Some(vec!["block_name.data".to_string()])
    );
}

#[test]
fn the_comparison_is_deterministic() {
    let (before, after) = renumbered(0.0);
    let once = diff(&before, &after, options(Matching::Reference));
    let again = diff(&before, &after, options(Matching::Reference));
    assert_eq!(once.to_json(false).unwrap(), again.to_json(false).unwrap());
}

/// G1 with one IMAGE in model space showing image definition `handle`,
/// which states `file`.
fn with_image(handle: &str, file: &str) -> CadDatabase {
    let mut v = g1();
    let image = json!({"type": "IMAGE", "common": {"id": 9000, "origin": "VECTOR",
        "confidence": "HIGH", "source_handle": {"type": "RESOLVED", "data": "2328"},
        "layer": {"type": "RESOLVED", "data": "0"}, "color_index": 256,
        "true_color": null, "invisible": false, "linetype": {"type": "BY_LAYER"},
        "linetype_scale": 1.0, "lineweight": -1, "transparency": null},
        "insertion_point": {"x": 10.0, "y": 20.0, "z": 0.0},
        "u_vector": {"x": 0.03, "y": 0.0, "z": 0.0},
        "v_vector": {"x": 0.0, "y": 0.03, "z": 0.0},
        "size_pixels": {"x": 300.0, "y": 168.0},
        "definition": {"type": "RESOLVED", "data": handle},
        "display_flags": 7, "clipping": false, "brightness": 50, "contrast": 50,
        "fade": 0, "clip_outside": false, "boundary": []});
    v["entities"].as_array_mut().unwrap().push(image.clone());
    v["tables"]["block_records"]["*Model_Space"]["entities"]
        .as_array_mut()
        .unwrap()
        .push(image);
    v["tables"]["image_definitions"] = json!({handle: {"file_path": file,
        "size_pixels": {"x": 300.0, "y": 168.0},
        "pixel_size": {"x": 0.0033333333333333, "y": 0.0033333333333333},
        "loaded": true, "resolution_unit": "UNITLESS"}});
    model(v)
}

fn image_fields(set: &ChangeSet) -> Option<Vec<String>> {
    set.changes.iter().find_map(|c| match c {
        Change::Modified(m) if m.entity_type == "IMAGE" => {
            Some(m.fields.iter().map(|f| f.path.clone()).collect())
        }
        _ => None,
    })
}

#[test]
fn an_image_definition_with_another_handle_and_the_same_file_is_no_change() {
    let before = with_image("80F", "parts/plate.png");
    let after = with_image("A0F", "parts/plate.png");
    for matching in [Matching::Reference, Matching::Geometry] {
        let set = diff(&before, &after, options(matching));
        assert!(
            set.changes.is_empty(),
            "{matching:?}: {:?}",
            set.changes.first()
        );
    }
}

#[test]
fn an_image_definition_that_names_another_file_keeps_both_handles() {
    let before = with_image("80F", "parts/plate.png");
    let after = with_image("A0F", "parts/bracket.png");
    let set = diff(&before, &after, options(Matching::Reference));
    assert_eq!(
        image_fields(&set),
        Some(vec!["definition.data".to_string()])
    );
    // Geometric matching pairs the image by its shape, which leaves the
    // handle out, and reports the same.
    let set = diff(&before, &after, options(Matching::Geometry));
    assert_eq!(
        image_fields(&set),
        Some(vec!["definition.data".to_string()])
    );
}
