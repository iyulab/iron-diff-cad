//! Field-by-field comparison of two entities through the model's own JSON
//! form.
//!
//! Every entity type serializes to a tree of objects, arrays and leaves;
//! comparing the two trees path by path gives the field list of a
//! `MODIFIED` change without a hand-written comparer per entity type -- and
//! it keeps the field paths identical to the ones a consumer sees in the
//! model's JSON, which is the point of stating them at all.

use crate::change_set::{FieldChange, Side, Tolerance, Verdict};
use serde_json::Value;
use std::collections::BTreeMap;

/// Field names the model documents as angles (radians). Every other numeric
/// field is compared with the length tolerance.
const ANGLE_FIELDS: [&str; 4] = ["rotation", "start_angle", "end_angle", "angle"];

/// Fields of `common` that are identity, not content: the reference ID is
/// the matching key under reference matching, and the source handle is
/// what the model issued that ID from. Neither is ever a compared field --
/// in an entity's own `common` block or in a nested entity's (an INSERT's
/// attributes).
const IDENTITY_FIELDS: [&str; 2] = ["id", "source_handle"];

/// Block references named by a save, not by the drawing, per entity type:
/// a dimension's anonymous block (`*D3`) is renumbered when the file is
/// saved again while the dimension stays as it was, and what that block
/// draws is what the dimension's own fields already say. Such a reference
/// is not part of the shape, and is not compared when both sides name an
/// anonymous block. A named block is the drawing's own word and stays both:
/// a dimension repointed to another named block, or between a named block
/// and an anonymous one, is a change.
const SAVE_ASSIGNED: [(&str, &str); 1] = [("DIMENSION", "block_name")];

/// Whether `value`, the field `key` of an entity of type `entity_type`, is
/// a block reference a save names ([`SAVE_ASSIGNED`]): a resolved reference
/// to an anonymous block, whose name starts with `*`.
fn save_assigned(entity_type: Option<&Value>, key: &str, value: Option<&Value>) -> bool {
    let entity_type = entity_type.and_then(Value::as_str);
    SAVE_ASSIGNED
        .iter()
        .any(|&(t, k)| Some(t) == entity_type && k == key)
        && value.is_some_and(|v| {
            v["type"] == "RESOLVED" && v["data"].as_str().is_some_and(|n| n.starts_with('*'))
        })
}

/// An entity's JSON form without its `common` block and type tag: the
/// geometry and values that say what the entity *is*, as opposed to where
/// it came from and what it is called. What geometric matching compares.
/// A nested entity (an INSERT's attributes) is part of the shape without
/// its own `common` block, for the same reason; so is a field a save names
/// ([`SAVE_ASSIGNED`]).
pub fn shape(entity: &Value) -> Value {
    match entity {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .filter(|(k, v)| {
                    k.as_str() != "common"
                        && k.as_str() != "type"
                        && !save_assigned(fields.get("type"), k, Some(v))
                })
                .map(|(k, v)| (k.clone(), without_common(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// `value` with every `common` block inside it left out. Every `common` in
/// the model's form is an entity's [`EntityCommon`](uncad_model::model::EntityCommon).
fn without_common(value: &Value) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .filter(|(k, _)| k.as_str() != "common")
                .map(|(k, v)| (k.clone(), without_common(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(without_common).collect()),
        other => other.clone(),
    }
}

/// The fields that differ between `before` and `after`, ordered by path.
pub fn compare(before: &Value, after: &Value, tolerance: Tolerance) -> Vec<FieldChange> {
    let mut out = BTreeMap::new();
    walk("", Some(before), Some(after), tolerance, &mut out);
    out.into_values().collect()
}

fn join(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else {
        format!("{prefix}.{key}")
    }
}

/// Compares the values at `path`. `None` is a side with no value there at
/// all -- no such key, or an array too short -- which is reported as `null`
/// but, unlike a `null` the model wrote, is not "not stated".
fn walk(
    path: &str,
    before_at: Option<&Value>,
    after_at: Option<&Value>,
    tolerance: Tolerance,
    out: &mut BTreeMap<String, FieldChange>,
) {
    let (before, after) = (
        before_at.unwrap_or(&Value::Null),
        after_at.unwrap_or(&Value::Null),
    );
    match (before, after) {
        (Value::Object(b), Value::Object(a)) => {
            // Identity fields are never compared -- see IDENTITY_FIELDS --
            // and nor is a reference both sides name an anonymous block by,
            // of an entity whose type both sides share (SAVE_ASSIGNED).
            let mut keys: Vec<&String> = b.keys().chain(a.keys()).collect();
            keys.sort();
            keys.dedup();
            let entity_type = b.get("type").filter(|t| a.get("type") == Some(*t));
            for key in keys {
                if path.is_empty()
                    && save_assigned(entity_type, key, b.get(key))
                    && save_assigned(entity_type, key, a.get(key))
                {
                    continue;
                }
                if key == "common" {
                    if let (Some(Value::Object(bc)), Some(Value::Object(ac))) =
                        (b.get(key), a.get(key))
                    {
                        let mut ck: Vec<&String> = bc.keys().chain(ac.keys()).collect();
                        ck.sort();
                        ck.dedup();
                        for c in ck {
                            if IDENTITY_FIELDS.contains(&c.as_str()) {
                                continue;
                            }
                            let at = join(&join(path, "common"), c);
                            walk(&at, bc.get(c), ac.get(c), tolerance, out);
                        }
                        continue;
                    }
                }
                walk(&join(path, key), b.get(key), a.get(key), tolerance, out);
            }
        }
        (Value::Array(b), Value::Array(a)) => {
            let n = b.len().max(a.len());
            for i in 0..n {
                walk(&format!("{path}[{i}]"), b.get(i), a.get(i), tolerance, out);
            }
        }
        (Value::Number(b), Value::Number(a)) => {
            let (Some(bv), Some(av)) = (b.as_f64(), a.as_f64()) else {
                return;
            };
            if bv == av {
                return;
            }
            let delta = (av - bv).abs();
            let tol = if is_angle(path) {
                tolerance.angle
            } else {
                tolerance.length
            };
            let verdict = if delta < tol {
                Verdict::Within
            } else {
                Verdict::Beyond
            };
            out.insert(
                path.to_string(),
                FieldChange {
                    path: path.to_string(),
                    before: before.clone(),
                    after: after.clone(),
                    delta: Some(delta),
                    tolerance: Some(tol),
                    verdict,
                    unstated: None,
                },
            );
        }
        _ => {
            if before != after {
                out.insert(
                    path.to_string(),
                    FieldChange {
                        path: path.to_string(),
                        before: before.clone(),
                        after: after.clone(),
                        delta: None,
                        tolerance: None,
                        verdict: Verdict::Beyond,
                        unstated: unstated(before_at, after_at),
                    },
                );
            }
        }
    }
}

/// The share of two shapes' leaf values that agree, by the rules
/// [`compare`] uses: a number agrees when it differs by less than its
/// tolerance, any other leaf when it is equal. Every leaf either shape
/// holds is counted once -- a field one side lacks, or an array element
/// past the shorter array's end, counts as leaves that disagree -- so the
/// longer shape sets the scale. An empty object or array counts as one
/// leaf.
pub fn similarity(before: &Value, after: &Value, tolerance: Tolerance) -> f64 {
    let (agree, total) = agreement("", Some(before), Some(after), tolerance);
    agree as f64 / total.max(1) as f64
}

/// `(leaves that agree, leaves)` at `path`.
fn agreement(
    path: &str,
    before: Option<&Value>,
    after: Option<&Value>,
    tolerance: Tolerance,
) -> (usize, usize) {
    match (before, after) {
        (None, None) => (0, 0),
        (Some(v), None) | (None, Some(v)) => (0, leaves(v)),
        (Some(Value::Object(b)), Some(Value::Object(a))) if !(b.is_empty() && a.is_empty()) => {
            let mut keys: Vec<&String> = b.keys().chain(a.keys()).collect();
            keys.sort();
            keys.dedup();
            keys.into_iter().fold((0, 0), |(s, n), k| {
                let (s2, n2) = agreement(&join(path, k), b.get(k), a.get(k), tolerance);
                (s + s2, n + n2)
            })
        }
        (Some(Value::Array(b)), Some(Value::Array(a))) if !(b.is_empty() && a.is_empty()) => {
            (0..b.len().max(a.len())).fold((0, 0), |(s, n), i| {
                let (s2, n2) = agreement(&format!("{path}[{i}]"), b.get(i), a.get(i), tolerance);
                (s + s2, n + n2)
            })
        }
        (Some(Value::Number(b)), Some(Value::Number(a))) => {
            let agrees = match (b.as_f64(), a.as_f64()) {
                (Some(bv), Some(av)) => {
                    let tol = if is_angle(path) {
                        tolerance.angle
                    } else {
                        tolerance.length
                    };
                    bv == av || (av - bv).abs() < tol
                }
                _ => b == a,
            };
            (usize::from(agrees), 1)
        }
        (Some(b), Some(a)) if is_leaf(b) && is_leaf(a) => (usize::from(b == a), 1),
        // A leaf against a container, or an object against an array.
        (Some(b), Some(a)) => (0, leaves(b).max(leaves(a))),
    }
}

/// A scalar, or an empty object or array.
fn is_leaf(v: &Value) -> bool {
    match v {
        Value::Object(m) => m.is_empty(),
        Value::Array(a) => a.is_empty(),
        _ => true,
    }
}

fn leaves(v: &Value) -> usize {
    match v {
        Value::Object(m) if !m.is_empty() => m.values().map(leaves).sum(),
        Value::Array(a) if !a.is_empty() => a.iter().map(leaves).sum(),
        _ => 1,
    }
}

/// Which side holds the model's `null` where the other holds a value; a
/// side with nothing there at all does not count.
fn unstated(before: Option<&Value>, after: Option<&Value>) -> Option<Side> {
    match (before, after) {
        (Some(Value::Null), Some(a)) if !a.is_null() => Some(Side::Before),
        (Some(b), Some(Value::Null)) if !b.is_null() => Some(Side::After),
        _ => None,
    }
}

fn is_angle(path: &str) -> bool {
    let last = path.rsplit('.').next().unwrap_or(path);
    let last = last.split('[').next().unwrap_or(last);
    ANGLE_FIELDS.contains(&last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_a_dimension_block_named_by_a_save_on_both_sides_is_left_out() {
        let dimension = |block: &str| {
            json!({"type": "DIMENSION", "measurement": 10.0,
                   "block_name": {"type": "RESOLVED", "data": block}})
        };
        let tol = Tolerance::default();
        // Anonymous on both sides: a save renumbered it.
        assert!(compare(&dimension("*D3"), &dimension("*D4"), tol).is_empty());
        assert_eq!(shape(&dimension("*D3")), shape(&dimension("*D4")));
        // A named block is the drawing's own word: repointing to or from one
        // is a change, compared and in the shape.
        for (b, a) in [("*D3", "SECTION_MARK"), ("SECTION_MARK", "DETAIL_MARK")] {
            let paths: Vec<String> = compare(&dimension(b), &dimension(a), tol)
                .into_iter()
                .map(|c| c.path)
                .collect();
            assert_eq!(paths, ["block_name.data"], "{b} -> {a}");
            assert_ne!(shape(&dimension(b)), shape(&dimension(a)));
        }
        // An INSERT's block is never left out, anonymous or not.
        let insert = |block: &str| json!({"type": "INSERT", "block_name": {"type": "RESOLVED", "data": block}});
        assert_eq!(compare(&insert("*U1"), &insert("*U2"), tol).len(), 1);
    }

    #[test]
    fn numeric_fields_get_a_delta_and_a_verdict_and_identity_is_never_compared() {
        let before = json!({"common": {"id": 1, "source_handle": {"type": "RESOLVED", "data": "1"}, "layer": {"type": "RESOLVED", "data": "0"}}, "radius": 5.0, "center": {"x": 1.0, "y": 2.0, "z": 0.0}});
        let after = json!({"common": {"id": 2, "source_handle": {"type": "RESOLVED", "data": "2"}, "layer": {"type": "RESOLVED", "data": "0"}}, "radius": 6.0, "center": {"x": 1.0, "y": 2.0000001, "z": 0.0}});
        let changes = compare(&before, &after, Tolerance::default());
        let paths: Vec<&str> = changes.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(
            paths,
            ["center.y", "radius"],
            "ordered by path, id and source handle skipped"
        );
        assert_eq!(changes[0].verdict, Verdict::Within);
        assert_eq!(changes[1].verdict, Verdict::Beyond);
        assert_eq!(changes[1].delta, Some(1.0));
    }

    #[test]
    fn angles_use_the_angle_tolerance_by_field_name() {
        let before = json!({"rotation": 0.0, "text_height": 0.0});
        let after = json!({"rotation": 1e-5, "text_height": 1e-5});
        let tol = Tolerance {
            length: 1e-3,
            angle: 1e-9,
        };
        let changes = compare(&before, &after, tol);
        assert_eq!(changes[0].path, "rotation");
        assert_eq!(changes[0].tolerance, Some(1e-9));
        assert_eq!(changes[0].verdict, Verdict::Beyond);
        assert_eq!(changes[1].path, "text_height");
        assert_eq!(changes[1].tolerance, Some(1e-3));
        assert_eq!(changes[1].verdict, Verdict::Within);
    }

    #[test]
    fn non_numeric_fields_are_listed_only_when_they_differ() {
        let before =
            json!({"text": "A", "closed": true, "layer": {"type": "RESOLVED", "data": "0"}});
        let after = json!({"text": "A", "closed": false, "layer": {"type": "ABSENT"}});
        let changes = compare(&before, &after, Tolerance::default());
        let paths: Vec<&str> = changes.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(paths, ["closed", "layer.data", "layer.type"]);
        assert!(changes
            .iter()
            .all(|c| c.delta.is_none() && c.verdict == Verdict::Beyond));
    }

    #[test]
    fn the_shape_is_everything_but_the_common_block_and_the_tag() {
        let entity = json!({"type": "CIRCLE", "common": {"id": 1, "layer": "0"}, "radius": 5.0, "center": {"x": 1.0, "y": 2.0, "z": 0.0}});
        assert_eq!(
            shape(&entity),
            json!({"radius": 5.0, "center": {"x": 1.0, "y": 2.0, "z": 0.0}})
        );
    }

    fn insert_with_attribute(id: u64, layer: &str) -> Value {
        json!({"type": "INSERT", "common": {"id": id, "source_handle": {"type": "RESOLVED", "data": format!("{id:X}")}},
               "block_name": "TITLE",
               "attribs": [{"common": {"id": id + 1, "source_handle": {"type": "RESOLVED", "data": format!("{:X}", id + 1)}, "layer": layer},
                            "tag": "PART", "value": "A-1"}]})
    }

    #[test]
    fn a_nested_entity_is_part_of_the_shape_without_its_common_block() {
        let shape = shape(&insert_with_attribute(1, "0"));
        assert_eq!(
            shape,
            json!({"block_name": "TITLE", "attribs": [{"tag": "PART", "value": "A-1"}]})
        );
        assert_eq!(shape, super::shape(&insert_with_attribute(0x500, "0")));
    }

    #[test]
    fn a_nested_entity_identity_is_never_a_compared_field_but_its_layer_is() {
        let tol = Tolerance::default();
        let before = insert_with_attribute(1, "0");
        assert!(compare(&before, &insert_with_attribute(0x500, "0"), tol)
            .iter()
            .all(|f| !f.path.starts_with("attribs")));
        let paths: Vec<String> = compare(&before, &insert_with_attribute(1, "NOTES"), tol)
            .into_iter()
            .map(|f| f.path)
            .collect();
        assert_eq!(paths, ["attribs[0].common.layer"]);
    }

    #[test]
    fn arrays_are_compared_by_index_and_a_missing_element_is_a_change() {
        let before = json!({"vertices": [{"x": 0.0, "y": 0.0}, {"x": 1.0, "y": 0.0}]});
        let after = json!({"vertices": [{"x": 0.0, "y": 0.0}]});
        let changes = compare(&before, &after, Tolerance::default());
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "vertices[1]");
        assert_eq!(changes[0].after, Value::Null);
        // An element that is not there is a change of shape, not a value
        // the file stopped stating.
        assert_eq!(changes[0].unstated, None);
    }

    #[test]
    fn a_value_one_side_does_not_state_is_marked_with_that_side() {
        let before = json!({"transparency": null, "closed": null, "target": {"x": 1.0}, "n": 1.0});
        let after = json!({"transparency": 0, "closed": false, "target": null, "n": null});
        let changes = compare(&before, &after, Tolerance::default());
        let marks: Vec<(&str, Option<Side>)> = changes
            .iter()
            .map(|c| (c.path.as_str(), c.unstated))
            .collect();
        assert_eq!(
            marks,
            [
                ("closed", Some(Side::Before)),
                ("n", Some(Side::After)),
                ("target", Some(Side::After)),
                ("transparency", Some(Side::Before)),
            ]
        );
        // Still a change beyond tolerance: nothing is folded into "the same".
        assert!(changes.iter().all(|c| c.verdict == Verdict::Beyond));
        // Two stated values carry no mark.
        let changes = compare(&json!({"r": 1.0}), &json!({"r": 2.0}), Tolerance::default());
        assert_eq!(changes[0].unstated, None);
    }

    #[test]
    fn similarity_is_the_share_of_leaves_that_agree() {
        let tol = Tolerance::default();
        let circle = |x: f64, r: f64| {
            json!({"center": {"x": x, "y": 2.0, "z": 0.0}, "radius": r,
                   "extrusion": {"x": 0.0, "y": 0.0, "z": 1.0}})
        };
        assert_eq!(similarity(&circle(1.0, 5.0), &circle(1.0, 5.0), tol), 1.0);
        assert_eq!(
            similarity(&circle(1.0, 5.0), &circle(1.0, 6.0), tol),
            6.0 / 7.0
        );
        // Within tolerance agrees, as it does for the field list.
        assert_eq!(
            similarity(&circle(1.0, 5.0), &circle(1.0 + 1e-7, 5.0), tol),
            1.0
        );
        assert_eq!(
            similarity(&circle(1.0, 5.0), &circle(9.0, 6.0), tol),
            5.0 / 7.0
        );
    }

    #[test]
    fn similarity_counts_what_one_side_lacks_against_the_longer_shape() {
        let tol = Tolerance::default();
        let point = |x: f64| json!({"x": x, "y": 0.0});
        let three = json!({"vertices": [point(0.0), point(1.0), point(2.0)]});
        let two = json!({"vertices": [point(0.0), point(1.0)]});
        // Six leaves on the longer side, four of them shared.
        assert_eq!(similarity(&three, &two, tol), 4.0 / 6.0);
        assert_eq!(similarity(&two, &three, tol), 4.0 / 6.0);
        // A field only one side has counts as its leaves.
        let tagged = json!({"vertices": [point(0.0), point(1.0)], "closed": true});
        assert_eq!(similarity(&two, &tagged, tol), 4.0 / 5.0);
        // A leaf against a container: the container's leaves, none agreeing.
        assert_eq!(
            similarity(&json!({"p": null}), &json!({"p": point(0.0)}), tol),
            0.0
        );
        // Empty containers are one leaf each.
        assert_eq!(similarity(&json!({"a": []}), &json!({"a": []}), tol), 1.0);
    }

    #[test]
    fn similarity_uses_the_angle_tolerance_by_field_name() {
        let tol = Tolerance {
            length: 1e-3,
            angle: 1e-9,
        };
        let before = json!({"rotation": 0.0, "text_height": 0.0});
        let after = json!({"rotation": 1e-5, "text_height": 1e-5});
        assert_eq!(similarity(&before, &after, tol), 0.5);
    }
}
