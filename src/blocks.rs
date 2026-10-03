//! Anonymous blocks compared by what they hold.
//!
//! A save numbers the anonymous blocks it writes (`*U24` in one file is
//! `*U96` in the next), so the name an INSERT (or a table) gives one says
//! nothing about the drawing. Unlike a dimension's block, what an INSERT
//! draws is not in its own fields: two INSERTs naming two anonymous blocks
//! draw the same thing exactly when the two blocks hold the same thing. That is what is
//! compared here, with the same field comparison and tolerance as every
//! other field -- no name is guessed from, and an INSERT repointed to an
//! anonymous block that holds something else keeps its `block_name` change,
//! both names stated.

use crate::change_set::{FieldChange, Tolerance, Verdict};
use crate::fields;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::BTreeMap;
use uncad_model::CadDatabase;

/// How deep anonymous blocks may nest inside one another before two of
/// them are no longer confirmed alike -- the same budget a placement's
/// expansion has elsewhere in the stack. Past it the names stay a change.
const MAX_DEPTH: usize = 20;

/// The path an INSERT's block name is reported under.
const BLOCK_NAME: &str = "block_name.data";

/// The anonymous blocks of two states, for settling whether an INSERT that
/// names one in each points at the same drawing.
pub(crate) struct AnonymousBlocks<'a> {
    before: &'a CadDatabase,
    after: &'a CadDatabase,
    tolerance: Tolerance,
    /// Pairs already compared: a block is often inserted many times.
    seen: RefCell<BTreeMap<(String, String), bool>>,
}

impl<'a> AnonymousBlocks<'a> {
    pub(crate) fn new(
        before: &'a CadDatabase,
        after: &'a CadDatabase,
        tolerance: Tolerance,
    ) -> Self {
        AnonymousBlocks {
            before,
            after,
            tolerance,
            seen: RefCell::new(BTreeMap::new()),
        }
    }

    /// `fields`, the field changes of an entity whose JSON forms are
    /// `before` and `after`, without the change of an INSERT's
    /// `block_name` from one anonymous block to another that holds the same
    /// entities.
    pub(crate) fn settle(
        &self,
        before: &Value,
        after: &Value,
        mut fields: Vec<FieldChange>,
    ) -> Vec<FieldChange> {
        if let Some((b, a)) = anonymous_pair(before, after) {
            if self.same(&b, &a, 0) {
                fields.retain(|f| f.path != BLOCK_NAME);
            }
        }
        fields
    }

    /// Whether the block `b` of the first state and `a` of the second hold
    /// the same entities, in the same order, within tolerance -- and the
    /// same base point. An entity of one that is an INSERT of an anonymous
    /// block may name a different one than its counterpart, if those two
    /// hold the same in turn.
    fn same(&self, b: &str, a: &str, depth: usize) -> bool {
        if depth > MAX_DEPTH {
            return false;
        }
        let key = (b.to_string(), a.to_string());
        if let Some(&known) = self.seen.borrow().get(&key) {
            return known;
        }
        let verdict = self.compare(b, a, depth);
        self.seen.borrow_mut().insert(key, verdict);
        verdict
    }

    fn compare(&self, b: &str, a: &str, depth: usize) -> bool {
        let (Some(bb), Some(ab)) = (
            self.before.tables.block_records.get(b),
            self.after.tables.block_records.get(a),
        ) else {
            return false;
        };
        if bb.entities.len() != ab.entities.len() {
            return false;
        }
        let point = |p| serde_json::to_value(p).expect("the model serializes");
        if !within(&fields::compare(
            &point(bb.base_point),
            &point(ab.base_point),
            self.tolerance,
        )) {
            return false;
        }
        bb.entities.iter().zip(&ab.entities).all(|(x, y)| {
            if x.type_name() != y.type_name() {
                return false;
            }
            let xv = serde_json::to_value(x).expect("the model serializes");
            let yv = serde_json::to_value(y).expect("the model serializes");
            let mut changes = fields::compare(&xv, &yv, self.tolerance);
            if let Some((nb, na)) = anonymous_pair(&xv, &yv) {
                if changes.iter().any(|f| f.path == BLOCK_NAME) {
                    if !self.same(&nb, &na, depth + 1) {
                        return false;
                    }
                    changes.retain(|f| f.path != BLOCK_NAME);
                }
            }
            within(&changes)
        })
    }
}

/// The two block names, when `before` and `after` are entities of one type
/// that draws through its block ([`fields::CONTENT_ADDRESSED`]) and both
/// name an anonymous block (a resolved reference whose name starts with
/// `*`).
fn anonymous_pair(before: &Value, after: &Value) -> Option<(String, String)> {
    if before["type"] != after["type"]
        || !fields::CONTENT_ADDRESSED
            .iter()
            .any(|&(t, k)| before["type"] == t && k == "block_name")
    {
        return None;
    }
    let name = |v: &Value| -> Option<String> {
        if v["block_name"]["type"] != "RESOLVED" {
            return None;
        }
        let n = v["block_name"]["data"].as_str()?;
        n.starts_with('*').then(|| n.to_string())
    };
    Some((name(before)?, name(after)?))
}

/// No field differs beyond tolerance.
fn within(changes: &[FieldChange]) -> bool {
    changes.iter().all(|f| f.verdict == Verdict::Within)
}
