//! Text fields whose strings differ only in formatting codes. A drawing
//! saved again may state a paragraph alignment or a font switch the older
//! save left out; the text reads the same. The change is reported -- the
//! stored strings differ -- and marked `format_only`.

use iron_diff_cad::{diff, Change, DiffOptions, FieldChange, Matching, Verdict};
use uncad_model::model::{Entity, EntityId, TextOverride};
use uncad_model::CadDatabase;

fn by_reference() -> DiffOptions {
    DiffOptions {
        matching: Matching::Reference,
        ..DiffOptions::default()
    }
}

fn g1() -> CadDatabase {
    serde_json::from_str(include_str!("golden/g1.expected.json"))
        .expect("the golden model deserializes")
}

/// g1 with one MTEXT added, whose text is `text`.
fn with_mtext(text: &str) -> CadDatabase {
    let mut db = g1();
    let mut common = serde_json::to_value(db.entities[0].common()).unwrap();
    common["id"] = serde_json::json!(9000);
    let mtext: Entity = serde_json::from_value(serde_json::json!({
        "type": "MTEXT",
        "common": common,
        "insertion_point": { "x": 0.0, "y": 0.0, "z": 0.0 },
        "text": text,
        "text_height": 2.5,
        "rotation": 0.0,
        "line_spacing_factor": 1.0,
        "attachment": "TOP_LEFT",
        "extents_width": null,
        "extents_height": null,
    }))
    .expect("an MTEXT deserializes");
    db.entities.push(mtext);
    db
}

/// The one field change of the one `MODIFIED` entry.
fn only_field(before: &CadDatabase, after: &CadDatabase) -> FieldChange {
    let set = diff(before, after, by_reference());
    assert_eq!(set.changes.len(), 1, "{:?}", set.changes);
    let Change::Modified(m) = &set.changes[0] else {
        panic!("a text edit is MODIFIED: {:?}", set.changes[0]);
    };
    assert_eq!(m.fields.len(), 1, "{:?}", m.fields);
    m.fields[0].clone()
}

#[test]
fn a_paragraph_code_added_on_a_save_is_format_only() {
    let f = only_field(
        &with_mtext(r"\A1;MATERIAL : A36 (9t)"),
        &with_mtext(r"\A1;\pxql;MATERIAL : A36 (9t)"),
    );
    assert_eq!(f.path, "text");
    assert!(f.format_only);
    assert_eq!(f.verdict, Verdict::Beyond, "the stored strings do differ");
}

#[test]
fn a_font_switch_and_a_group_are_format_only() {
    let f = only_field(&with_mtext("20EA"), &with_mtext(r"{\fArial|b1;20EA}"));
    assert!(f.format_only);
}

#[test]
fn a_changed_character_is_not_format_only() {
    let f = only_field(
        &with_mtext(r"\A1;\pxql;20EA"),
        &with_mtext(r"\A1;\pxql;8EA"),
    );
    assert!(!f.format_only);
    let json = serde_json::to_string(&f).unwrap();
    assert!(!json.contains("format_only"), "absent when false: {json}");
}

#[test]
fn a_paragraph_break_is_content_not_formatting() {
    let f = only_field(&with_mtext("A B"), &with_mtext(r"A\PB"));
    assert!(!f.format_only);
}

#[test]
fn an_underline_toggle_on_a_text_is_format_only() {
    let before = g1();
    let mut after = g1();
    let id = text_id(&before);
    for e in all_mut(&mut after) {
        if let Entity::Text(t) = e {
            if t.common.id == id {
                t.text = format!("%%u{}", t.text);
            }
        }
    }
    let f = only_field(&before, &after);
    assert_eq!(f.path, "text");
    assert!(f.format_only);
}

#[test]
fn a_dimension_text_override_written_with_a_group_is_format_only() {
    let mut before = g1();
    let mut after = g1();
    let id = dimension_id(&before);
    set_override(&mut before, id, "<> H7");
    set_override(&mut after, id, r"{\H1.0x;<> H7}");
    let f = only_field(&before, &after);
    assert_eq!(f.path, "text_override.data");
    assert!(f.format_only);
}

fn all_mut(db: &mut CadDatabase) -> impl Iterator<Item = &mut Entity> {
    db.entities.iter_mut().chain(
        db.tables
            .block_records
            .values_mut()
            .flat_map(|b| b.entities.iter_mut()),
    )
}

fn text_id(db: &CadDatabase) -> EntityId {
    db.tables
        .block_records
        .values()
        .flat_map(|b| b.entities.iter())
        .find(|e| e.type_name() == "TEXT")
        .expect("g1 has a TEXT")
        .common()
        .id
}

fn dimension_id(db: &CadDatabase) -> EntityId {
    db.entities
        .iter()
        .find(|e| e.type_name() == "DIMENSION")
        .expect("g1 has a DIMENSION")
        .common()
        .id
}

fn set_override(db: &mut CadDatabase, id: EntityId, text: &str) {
    for e in all_mut(db) {
        if let Entity::Dimension(d) = e {
            if d.common.id == id {
                d.text_override = TextOverride::Literal(text.to_string());
            }
        }
    }
}
