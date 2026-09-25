//! A projection of a change set: what it leaves out, and that it says so.

use iron_diff_cad::{diff, Change, ChangeSet, DiffOptions, Omit, Omitted, Side};
use uncad_model::model::Entity;
use uncad_model::CadDatabase;

fn g1() -> CadDatabase {
    serde_json::from_str(include_str!("golden/g1.expected.json"))
        .expect("the golden model deserializes")
}

/// G1 with every circle moved a hair (within the default tolerance), every
/// circle's radius grown (beyond it), and every entity's transparency
/// stated where G1 states none.
fn edited() -> (CadDatabase, CadDatabase) {
    let before = g1();
    let mut after = before.clone();
    for e in &mut after.entities {
        if let Entity::Circle(c) = e {
            c.center.x += 1e-9;
            c.radius += 1.0;
        }
        e.common_mut().transparency = Some(0);
    }
    assert!(before
        .entities
        .iter()
        .any(|e| e.common().transparency.is_none()));
    (before, after)
}

fn fields(set: &ChangeSet) -> Vec<(String, Option<Side>)> {
    set.changes
        .iter()
        .flat_map(|c| match c {
            Change::Modified(m) => m.fields.clone(),
            _ => Vec::new(),
        })
        .map(|f| (f.path, f.unstated))
        .collect()
}

#[test]
fn a_change_set_as_diff_returns_it_leaves_nothing_out() {
    let (before, after) = edited();
    let full = diff(&before, &after, DiffOptions::default());
    assert_eq!(full.omitted, None);
    let json = full.to_json(false).unwrap();
    assert!(!json.contains("omitted"), "absent from the JSON too");
}

#[test]
fn leaving_out_within_and_unstated_keeps_only_the_edits_and_counts_the_rest() {
    let (before, after) = edited();
    let full = diff(&before, &after, DiffOptions::default());
    let circles = before
        .entities
        .iter()
        .filter(|e| matches!(e, Entity::Circle(_)))
        .count();
    let unstated = fields(&full)
        .iter()
        .filter(|(_, u)| *u == Some(Side::Before))
        .count();
    assert!(
        unstated > circles,
        "the transparency of more than the circles"
    );

    let only = full.without(Omit {
        within: true,
        unstated: true,
    });
    let kept = fields(&only);
    assert_eq!(kept.len(), circles, "{kept:?}");
    assert!(kept.iter().all(|(p, u)| p == "radius" && u.is_none()));
    let modified = full
        .changes
        .iter()
        .filter(|c| matches!(c, Change::Modified(_)))
        .count();
    assert_eq!(
        only.omitted,
        Some(Omitted {
            within_fields: circles,
            unstated_fields: unstated,
            entities: modified - circles,
        })
    );
    // The projection is new; the change set it came from is unchanged.
    assert_eq!(full.omitted, None);
    assert_eq!(full, diff(&before, &after, DiffOptions::default()));
}

#[test]
fn a_projection_of_a_projection_adds_to_the_counts() {
    let (before, after) = edited();
    let full = diff(&before, &after, DiffOptions::default());
    let once = full
        .without(Omit {
            within: true,
            unstated: false,
        })
        .without(Omit {
            within: false,
            unstated: true,
        });
    let both = full.without(Omit {
        within: true,
        unstated: true,
    });
    assert_eq!(once, both);
    // Leaving out nothing still says that it was projected.
    assert_eq!(
        full.without(Omit::default()).omitted,
        Some(Omitted::default())
    );
}
