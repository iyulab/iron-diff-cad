//! Field-by-field comparison of two entities through the model's own JSON
//! form.
//!
//! Every entity type serializes to a tree of objects, arrays and leaves;
//! comparing the two trees path by path gives the field list of a
//! `MODIFIED` change without a hand-written comparer per entity type -- and
//! it keeps the field paths identical to the ones a consumer sees in the
//! model's JSON, which is the point of stating them at all.

use crate::change_set::{FieldChange, Tolerance, Verdict};
use serde_json::Value;
use std::collections::BTreeMap;

/// Field names the model documents as angles (radians). Every other numeric
/// field is compared with the length tolerance.
const ANGLE_FIELDS: [&str; 4] = ["rotation", "start_angle", "end_angle", "angle"];

/// Fields of `common` that are identity, not content: the reference ID is
/// the matching key under reference matching, and the source handle is
/// what the model issued that ID from. Neither is ever a compared field.
const IDENTITY_FIELDS: [&str; 2] = ["id", "source_handle"];

/// An entity's JSON form without its `common` block and type tag: the
/// geometry and values that say what the entity *is*, as opposed to where
/// it came from and what it is called. What geometric matching compares.
pub fn shape(entity: &Value) -> Value {
    match entity {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .filter(|(k, _)| k.as_str() != "common" && k.as_str() != "type")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The fields that differ between `before` and `after`, ordered by path.
pub fn compare(before: &Value, after: &Value, tolerance: Tolerance) -> Vec<FieldChange> {
    let mut out = BTreeMap::new();
    walk("", before, after, tolerance, &mut out);
    out.into_values().collect()
}

fn join(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else {
        format!("{prefix}.{key}")
    }
}

fn walk(
    path: &str,
    before: &Value,
    after: &Value,
    tolerance: Tolerance,
    out: &mut BTreeMap<String, FieldChange>,
) {
    match (before, after) {
        (Value::Object(b), Value::Object(a)) => {
            // Identity fields are never compared -- see IDENTITY_FIELDS.
            let mut keys: Vec<&String> = b.keys().chain(a.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                if path.is_empty() && key == "common" {
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
                            walk(
                                &join("common", c),
                                bc.get(c).unwrap_or(&Value::Null),
                                ac.get(c).unwrap_or(&Value::Null),
                                tolerance,
                                out,
                            );
                        }
                        continue;
                    }
                }
                walk(
                    &join(path, key),
                    b.get(key).unwrap_or(&Value::Null),
                    a.get(key).unwrap_or(&Value::Null),
                    tolerance,
                    out,
                );
            }
        }
        (Value::Array(b), Value::Array(a)) => {
            let n = b.len().max(a.len());
            for i in 0..n {
                walk(
                    &format!("{path}[{i}]"),
                    b.get(i).unwrap_or(&Value::Null),
                    a.get(i).unwrap_or(&Value::Null),
                    tolerance,
                    out,
                );
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
                    },
                );
            }
        }
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

    #[test]
    fn arrays_are_compared_by_index_and_a_missing_element_is_a_change() {
        let before = json!({"vertices": [{"x": 0.0, "y": 0.0}, {"x": 1.0, "y": 0.0}]});
        let after = json!({"vertices": [{"x": 0.0, "y": 0.0}]});
        let changes = compare(&before, &after, Tolerance::default());
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "vertices[1]");
        assert_eq!(changes[0].after, Value::Null);
    }
}
