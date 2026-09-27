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
    /// See [`anchor`].
    anchor: Option<[f64; 2]>,
}

impl<'a> Item<'a> {
    fn new(id: EntityId, entity: &'a Entity) -> Self {
        let full = serde_json::to_value(entity).expect("the model serializes");
        let shape = fields::shape(&full);
        let key = SortKey::of(entity.type_name(), &full);
        let anchor = anchor(&full, &shape);
        Item {
            id,
            entity,
            full,
            shape,
            key,
            anchor,
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

/// A `{x, y}` or `{x, y, z}` object as a point; `z` defaults to 0. A
/// polyline vertex (`{point, bulge}`) is its point.
fn point(v: &Value) -> Option<[f64; 3]> {
    if let Some(p) = v.get("point").filter(|p| p.is_object()) {
        return point(p);
    }
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

/// For each entity of `b`, the indices (ascending) of the entities of `a`
/// whose shape agrees with it.
fn candidates(b: &[Item<'_>], a: &[Item<'_>], tolerance: Tolerance) -> Vec<Vec<usize>> {
    let window = Window::new(a, tolerance);
    b.iter()
        .map(|x| {
            let mut js: Vec<usize> = window
                .around(x)
                .into_iter()
                .filter(|&j| same_shape(x, &a[j], tolerance))
                .collect();
            js.sort_unstable();
            js
        })
        .collect()
}

/// Where an entity is, for narrowing the search for its counterparts: its
/// representative point when it has one (see [`SortKey`]), otherwise the
/// first point its shape holds, in the shape's own field order, passing
/// over `extrusion` -- a direction, the same for nearly everything. `None`
/// when the shape holds no point at all.
///
/// The choice depends only on which fields the entity has and which of
/// them hold numbers, never on the numbers: two shapes that agree have the
/// same fields, so they take the same point, and it agrees within the
/// length tolerance like every other length field.
fn anchor(full: &Value, shape: &Value) -> Option<[f64; 2]> {
    fn first_point(v: &Value) -> Option<[f64; 3]> {
        match v {
            Value::Object(m) => point(v).or_else(|| {
                m.iter()
                    .filter(|(k, _)| k.as_str() != "extrusion")
                    .find_map(|(_, c)| first_point(c))
            }),
            Value::Array(a) => a.iter().find_map(first_point),
            _ => None,
        }
    }
    REPRESENTATIVE_FIELDS
        .iter()
        .find_map(|f| point(full.get(f)?))
        .or_else(|| {
            REPRESENTATIVE_ARRAYS
                .iter()
                .find_map(|f| point(first_element(full.get(f)?)?))
        })
        .or_else(|| first_point(shape))
        .map(|p| [p[0], p[1]])
}

/// The second state's entities ordered by type and then by where they are,
/// for finding the ones a shape could agree with without comparing it to
/// every entity. Everything is sorted, nothing hashed: the candidates do
/// not depend on anything but the two states.
struct Window {
    /// Entities with an [`anchor`]: `(type, x, y, index)`, sorted by type,
    /// then `x`, then `y`. (JSON numbers are finite, so no anchor is NaN.)
    anchored: Vec<(String, f64, f64, usize)>,
    /// Entities without one: `(type, index)`, sorted by type.
    loose: Vec<(String, usize)>,
    /// How far apart two agreeing coordinates can be: the length tolerance,
    /// or 0 when that is not a positive number (then only equal values
    /// agree).
    radius: f64,
}

impl Window {
    fn new(items: &[Item<'_>], tolerance: Tolerance) -> Self {
        let mut anchored = Vec::new();
        let mut loose = Vec::new();
        for (j, y) in items.iter().enumerate() {
            let ty = y.key.entity_type.clone();
            match y.anchor {
                Some([ax, ay]) => anchored.push((ty, ax, ay, j)),
                None => loose.push((ty, j)),
            }
        }
        anchored.sort_by(|p, q| {
            p.0.cmp(&q.0)
                .then(p.1.total_cmp(&q.1))
                .then(p.2.total_cmp(&q.2))
                .then(p.3.cmp(&q.3))
        });
        loose.sort();
        let radius = if tolerance.length > 0.0 {
            tolerance.length
        } else {
            0.0
        };
        Window {
            anchored,
            loose,
            radius,
        }
    }

    /// The entities of the same type as `x` that can agree with it: those
    /// anchored within the radius of its anchor on both axes (bounds
    /// included, ordinary comparisons, so -0 and 0 are the same place), or
    /// -- for an entity with no anchor -- every one of its type with none.
    fn around(&self, x: &Item<'_>) -> Vec<usize> {
        let t = x.key.entity_type.as_str();
        let Some([vx, vy]) = x.anchor else {
            let start = self.loose.partition_point(|(ty, _)| ty.as_str() < t);
            let end = self.loose.partition_point(|(ty, _)| ty.as_str() <= t);
            return self.loose[start..end].iter().map(|&(_, j)| j).collect();
        };
        let r = self.radius;
        let (lo, hi) = (vx - r, vx + r);
        let start = self
            .anchored
            .partition_point(|(ty, ax, _, _)| ty.as_str() < t || (ty.as_str() == t && *ax < lo));
        let end = self
            .anchored
            .partition_point(|(ty, ax, _, _)| ty.as_str() < t || (ty.as_str() == t && *ax <= hi));
        // Within the x window, the entries that share one x are sorted by y
        // (a column of a drawing's grid can be long): search each such run
        // for the y window instead of walking it.
        let window = &self.anchored[start..end.max(start)];
        let mut out = Vec::new();
        let mut i = 0;
        while i < window.len() {
            let x0 = window[i].1;
            let run = &window[i..];
            let run = &run[..run.partition_point(|e| e.1.total_cmp(&x0) == Ordering::Equal)];
            let from = run.partition_point(|e| e.2 < vy - r);
            let to = run.partition_point(|e| e.2 <= vy + r);
            out.extend(run[from..to.max(from)].iter().map(|e| e.3));
            i += run.len();
        }
        out
    }
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

    // Candidates in both directions. Two shapes that agree agree on their
    // representative point, a length field compared like every other, so
    // only the entities of the second state whose type is the same and
    // whose representative x lies within the length tolerance can be
    // candidates: a window over the second state sorted by (type, x). The
    // sort is total and involves no hash, so the result does not depend on
    // anything but the two states.
    let candidates_b = candidates(&b, &a, tolerance);
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
        omitted: None,
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

    /// The same over every golden model's entities -- dimensions, hatches,
    /// attributes, polylines, the kinds whose place is not their
    /// representative point -- matched against the model itself and
    /// against a copy with every entity moved by less and by more than the
    /// length tolerance.
    #[test]
    fn the_window_finds_the_candidates_every_pair_would_in_every_golden_model() {
        let models = [
            include_str!("../tests/golden/g1.expected.json"),
            include_str!("../tests/golden/g5.expected.json"),
            include_str!("../tests/golden/g6.expected.json"),
        ];
        let tolerance = Tolerance::default();
        for (m, json) in models.iter().enumerate() {
            let db: CadDatabase = serde_json::from_str(json).unwrap();
            let items: Vec<Item<'_>> = crate::reference::by_id(&db)
                .into_iter()
                .map(|(id, e)| Item::new(id, e))
                .collect();
            for shift in [0.0, 0.4 * tolerance.length, 3.0 * tolerance.length] {
                let mut moved = serde_json::to_value(&db).unwrap();
                nudge(&mut moved, shift);
                let moved: CadDatabase = serde_json::from_value(moved).unwrap();
                let others: Vec<Item<'_>> = crate::reference::by_id(&moved)
                    .into_iter()
                    .map(|(id, e)| Item::new(id, e))
                    .collect();
                let every_pair: Vec<Vec<usize>> = items
                    .iter()
                    .map(|x| {
                        (0..others.len())
                            .filter(|&j| same_shape(x, &others[j], tolerance))
                            .collect()
                    })
                    .collect();
                assert_eq!(
                    candidates(&items, &others, tolerance),
                    every_pair,
                    "model {m}, shift {shift}"
                );
            }
        }
    }

    /// Adds `by` to every `x` and `y` of every point object in `v`.
    fn nudge(v: &mut serde_json::Value, by: f64) {
        match v {
            serde_json::Value::Object(m) => {
                for key in ["x", "y"] {
                    if let Some(n) = m.get(key).and_then(serde_json::Value::as_f64) {
                        if m.contains_key("x") && m.contains_key("y") {
                            m.insert(key.to_string(), serde_json::Value::from(n + by));
                        }
                    }
                }
                for c in m.values_mut() {
                    nudge(c, by);
                }
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(|c| nudge(c, by)),
            _ => {}
        }
    }

    /// The window finds exactly the candidates comparing every pair would:
    /// lines whose start x (the representative point) sits at signed zeros,
    /// either side of the tolerance, at large magnitudes and at values the
    /// JSON form cannot hold (NaN, infinity -- written as null), under
    /// tolerances that are ordinary, zero, negative, not a number and
    /// infinite.
    #[test]
    fn the_window_finds_the_candidates_every_pair_would() {
        let g1: CadDatabase =
            serde_json::from_str(include_str!("../tests/golden/g1.expected.json")).unwrap();
        // A dimension's block draws lines.
        let template = g1
            .tables
            .block_records
            .values()
            .flat_map(|b| &b.entities)
            .find_map(|e| match e {
                Entity::Line(l) => Some(l.clone()),
                _ => None,
            })
            .expect("G1 has a line");
        let xs = [
            0.0,
            -0.0,
            5e-7,
            -5e-7,
            1e-6,
            1.5e-6,
            2e-6,
            1e9,
            1e9 + 5e-7,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ];
        let entities: Vec<Entity> = xs
            .iter()
            .flat_map(|&x| {
                [0.0, 5e-7].map(|dy| {
                    let mut l = template.clone();
                    l.start_point.x = x;
                    l.start_point.y += dy;
                    Entity::Line(l)
                })
            })
            .collect();
        let items: Vec<Item<'_>> = entities
            .iter()
            .enumerate()
            .map(|(i, e)| Item::new(EntityId::new(i as u64 + 1), e))
            .collect();
        for length in [1e-6, 0.0, -1.0, f64::NAN, f64::INFINITY] {
            let tolerance = Tolerance {
                length,
                ..Tolerance::default()
            };
            let every_pair: Vec<Vec<usize>> = items
                .iter()
                .map(|x| {
                    (0..items.len())
                        .filter(|&j| same_shape(x, &items[j], tolerance))
                        .collect()
                })
                .collect();
            assert_eq!(
                candidates(&items, &items, tolerance),
                every_pair,
                "length tolerance {length}"
            );
        }
    }

    #[test]
    fn the_representative_point_is_the_first_point_field_present() {
        let line = json!({"start_point": {"x": 1.0, "y": 2.0, "z": 3.0}, "end_point": {"x": 4.0, "y": 5.0, "z": 6.0}});
        let key = SortKey::of("LINE", &line);
        assert_eq!(key.representative, [1.0, 2.0, 3.0]);
        assert_eq!(key.second, [4.0, 5.0, 6.0]);

        // A polyline's vertices as the model writes them -- built from the
        // model's own type, so a change to its shape cannot leave this test
        // checking a shape nothing produces any more.
        let vertices = serde_json::to_value(vec![
            uncad_model::PolylineVertex {
                point: uncad_model::Point2D { x: 7.0, y: 8.0 },
                bulge: 0.5,
                ..uncad_model::PolylineVertex::default()
            },
            uncad_model::PolylineVertex::straight(uncad_model::Point2D { x: 9.0, y: 10.0 }),
        ])
        .unwrap();
        let polyline = json!({"vertices": vertices, "closed": true});
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
