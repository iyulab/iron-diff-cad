//! Matching by geometry: the two states share no references (two revisions
//! of a drawing), so an entity's counterpart is looked for by type and
//! shape -- and only a correspondence that is certain is used.
//!
//! The rule is mutual uniqueness: `x` of the first state and `y` of the
//! second are counterparts when `y` is the only entity of the second state
//! whose type and shape equal `x`'s within tolerance, *and* `x` is the only
//! entity of the first state that equals `y`. Anything less certain is
//! `UNKNOWN`, with every candidate listed -- a likeliest pair is never
//! picked, because a confident wrong match would hide a real change. An
//! entity with no candidate at all is `REMOVED` or `ADDED`, which is what a
//! move looks like here.

use crate::change_set::{Change, ChangeSet, Matching, Modified, Tolerance, Unknown, Verdict};
use crate::fields;
use crate::reference::{by_id, record};
use serde_json::Value;
use std::cmp::Ordering;
use uncad_model::model::{Entity, EntityId};
use uncad_model::CadDatabase;

/// One entity of one state, with the JSON forms the matching works on.
struct Item<'a> {
    id: EntityId,
    entity: &'a Entity,
    /// The whole entity, for the field list of a `MODIFIED` entry.
    full: Value,
    /// The entity without its `common` block: what "shape" means here.
    shape: Value,
    key: SortKey,
}

impl<'a> Item<'a> {
    fn new(id: EntityId, entity: &'a Entity) -> Self {
        let full = serde_json::to_value(entity).expect("the model serializes");
        let shape = fields::shape(&full);
        let key = SortKey::of(entity.type_name(), &full);
        Item {
            id,
            entity,
            full,
            shape,
            key,
        }
    }
}

/// The order of a change set in this mode (contract, section 4): entity
/// type, then the representative point, then the second point where the
/// type has one. Points are compared lexicographically on (x, y, z).
#[derive(Debug, Clone, PartialEq)]
struct SortKey {
    entity_type: String,
    representative: [f64; 3],
    second: [f64; 3],
}

/// Field names that hold an entity's representative point, in the order
/// they are tried; the first one present wins. An array field contributes
/// its first element.
const REPRESENTATIVE_FIELDS: [&str; 7] = [
    "start_point",
    "center",
    "position",
    "insertion_point",
    "point",
    "corner1",
    "base_point",
];
const REPRESENTATIVE_ARRAYS: [&str; 6] = [
    "vertices",
    "fit_points",
    "control_points",
    "boundary",
    "lines",
    "wireframe_edges",
];
/// The second point, where the type has one.
const SECOND_FIELDS: [&str; 5] = [
    "end_point",
    "major_axis_endpoint",
    "vector",
    "corner2",
    "target",
];

impl SortKey {
    fn of(entity_type: &str, full: &Value) -> Self {
        SortKey {
            entity_type: entity_type.to_string(),
            representative: REPRESENTATIVE_FIELDS
                .iter()
                .find_map(|f| point(full.get(f)?))
                .or_else(|| {
                    REPRESENTATIVE_ARRAYS
                        .iter()
                        .find_map(|f| point(first_element(full.get(f)?)?))
                })
                .unwrap_or([0.0; 3]),
            second: SECOND_FIELDS
                .iter()
                .find_map(|f| point(full.get(f)?))
                .or_else(|| {
                    REPRESENTATIVE_ARRAYS
                        .iter()
                        .find_map(|f| point(full.get(f)?.as_array()?.get(1)?))
                })
                .unwrap_or([0.0; 3]),
        }
    }

    fn cmp(&self, other: &Self) -> Ordering {
        self.entity_type
            .cmp(&other.entity_type)
            .then_with(|| cmp_point(&self.representative, &other.representative))
            .then_with(|| cmp_point(&self.second, &other.second))
    }
}

/// A `{x, y}` or `{x, y, z}` object as a point; `z` defaults to 0.
fn point(v: &Value) -> Option<[f64; 3]> {
    let x = v.get("x")?.as_f64()?;
    let y = v.get("y")?.as_f64()?;
    let z = v.get("z").and_then(Value::as_f64).unwrap_or(0.0);
    Some([x, y, z])
}

/// The first point-like element of an array, descending into nested arrays
/// (a leader's polylines are arrays of arrays of points).
fn first_element(v: &Value) -> Option<&Value> {
    let first = v.as_array()?.first()?;
    if first.is_array() {
        first_element(first)
    } else {
        Some(first)
    }
}

fn cmp_point(a: &[f64; 3], b: &[f64; 3]) -> Ordering {
    a.iter()
        .zip(b)
        .map(|(p, q)| p.total_cmp(q))
        .find(|o| *o != Ordering::Equal)
        .unwrap_or(Ordering::Equal)
}

/// `true` when the two shapes are the same type and every field agrees
/// within tolerance: no field differs beyond it, and no non-numeric field
/// differs at all.
fn same_shape(x: &Item<'_>, y: &Item<'_>, tolerance: Tolerance) -> bool {
    x.entity.type_name() == y.entity.type_name()
        && fields::compare(&x.shape, &y.shape, tolerance)
            .iter()
            .all(|f| f.verdict == Verdict::Within)
}

/// The change set between two states that share no references. Every
/// entity of each state is either the certain counterpart of exactly one
/// entity of the other (compared field by field), or has no candidate
/// (`REMOVED`/`ADDED`), or is `UNKNOWN` with its candidates.
pub fn diff(before: &CadDatabase, after: &CadDatabase, tolerance: Tolerance) -> ChangeSet {
    let b: Vec<Item<'_>> = by_id(before)
        .into_iter()
        .map(|(id, e)| Item::new(id, e))
        .collect();
    let a: Vec<Item<'_>> = by_id(after)
        .into_iter()
        .map(|(id, e)| Item::new(id, e))
        .collect();

    // Candidates in both directions. Quadratic in the entity count; a
    // drawing is small enough, and nothing here may depend on a hash.
    let candidates_b: Vec<Vec<usize>> = b
        .iter()
        .map(|x| {
            a.iter()
                .enumerate()
                .filter(|(_, y)| same_shape(x, y, tolerance))
                .map(|(j, _)| j)
                .collect()
        })
        .collect();
    let mut candidates_a: Vec<Vec<usize>> = vec![Vec::new(); a.len()];
    for (i, js) in candidates_b.iter().enumerate() {
        for &j in js {
            candidates_a[j].push(i);
        }
    }

    let mut changes: Vec<(SortKey, Change)> = Vec::new();
    for (i, x) in b.iter().enumerate() {
        let js = &candidates_b[i];
        match js.as_slice() {
            [] => changes.push((x.key.clone(), Change::Removed(record(x.entity)))),
            [j] if candidates_a[*j].len() == 1 => {
                let y = &a[*j];
                let fields = fields::compare(&x.full, &y.full, tolerance);
                if fields.is_empty() {
                    continue;
                }
                changes.push((
                    x.key.clone(),
                    Change::Modified(Modified {
                        id: x.id,
                        counterpart: Some(y.id),
                        entity_type: y.entity.type_name().to_string(),
                        provenance: [x.entity.common().origin, y.entity.common().origin],
                        confidence: x
                            .entity
                            .common()
                            .confidence
                            .min(y.entity.common().confidence),
                        fields,
                    }),
                ));
            }
            [j] => {
                let others = candidates_a[*j].len() - 1;
                changes.push((
                    x.key.clone(),
                    Change::Unknown(Unknown {
                        id: x.id,
                        candidates: vec![a[*j].id],
                        reason: format!(
                            "its only candidate in the second state also matches {others} other \
                             {} of the first state within tolerance",
                            plural(others, "entity", "entities")
                        ),
                    }),
                ));
            }
            many => {
                let mut candidates: Vec<EntityId> = many.iter().map(|&j| a[j].id).collect();
                candidates.sort();
                changes.push((
                    x.key.clone(),
                    Change::Unknown(Unknown {
                        id: x.id,
                        candidates,
                        reason: format!(
                            "{} {} of the second state match this {} within tolerance",
                            many.len(),
                            plural(many.len(), "entity", "entities"),
                            x.entity.type_name()
                        ),
                    }),
                ));
            }
        }
    }
    for (j, y) in a.iter().enumerate() {
        if candidates_a[j].is_empty() {
            changes.push((y.key.clone(), Change::Added(record(y.entity))));
        }
    }

    // Stable: entries with equal keys keep the order they were found in
    // (first state by reference ID, then the second state's additions).
    changes.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

    ChangeSet {
        matching: Matching::Geometry,
        tolerance,
        changes: changes.into_iter().map(|(_, c)| c).collect(),
    }
}

fn plural(n: usize, one: &'static str, many: &'static str) -> &'static str {
    if n == 1 {
        one
    } else {
        many
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_representative_point_is_the_first_point_field_present() {
        let line = json!({"start_point": {"x": 1.0, "y": 2.0, "z": 3.0}, "end_point": {"x": 4.0, "y": 5.0, "z": 6.0}});
        let key = SortKey::of("LINE", &line);
        assert_eq!(key.representative, [1.0, 2.0, 3.0]);
        assert_eq!(key.second, [4.0, 5.0, 6.0]);

        let polyline =
            json!({"vertices": [{"x": 7.0, "y": 8.0}, {"x": 9.0, "y": 10.0}], "closed": true});
        let key = SortKey::of("LWPOLYLINE", &polyline);
        assert_eq!(key.representative, [7.0, 8.0, 0.0]);
        assert_eq!(key.second, [9.0, 10.0, 0.0]);

        let dimension = json!({"block_name": {"type": "RESOLVED", "data": "*D1"}});
        let key = SortKey::of("DIMENSION", &dimension);
        assert_eq!(key.representative, [0.0; 3]);
    }

    #[test]
    fn keys_order_by_type_then_point_then_second_point() {
        let k = |t: &str, r: [f64; 3], s: [f64; 3]| SortKey {
            entity_type: t.to_string(),
            representative: r,
            second: s,
        };
        assert_eq!(
            k("CIRCLE", [9.0; 3], [0.0; 3]).cmp(&k("LINE", [0.0; 3], [0.0; 3])),
            Ordering::Less
        );
        assert_eq!(
            k("LINE", [0.0, 1.0, 0.0], [0.0; 3]).cmp(&k("LINE", [0.0, 0.0, 5.0], [0.0; 3])),
            Ordering::Greater
        );
        assert_eq!(
            k("LINE", [0.0; 3], [1.0, 0.0, 0.0]).cmp(&k("LINE", [0.0; 3], [2.0, 0.0, 0.0])),
            Ordering::Less
        );
    }
}
