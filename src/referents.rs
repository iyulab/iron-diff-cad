//! References compared by what they point at.
//!
//! Some fields of an entity name another object of the drawing rather than
//! saying something themselves, and the name is the save's, not the
//! drawing's: a save numbers the anonymous blocks it writes (`*U24` in one
//! file is `*U96` in the next), and an IMAGE names its IMAGEDEF by handle.
//! What such an entity draws is in the object -- the block's entities, the
//! image definition's file and size -- so two of them draw the same exactly
//! when the two objects hold the same. That is what is compared here, with
//! the same field comparison and tolerance as every other field. Nothing is
//! guessed from a name, and an entity pointed at an object that holds
//! something else keeps its change, both names stated.

use crate::change_set::{FieldChange, Tolerance, Verdict};
use crate::fields::{self, Referent};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::BTreeMap;
use uncad_model::CadDatabase;

/// How deep anonymous blocks may nest inside one another before two of
/// them are no longer confirmed alike -- the same budget a placement's
/// expansion has elsewhere in the stack. Past it the names stay a change.
const MAX_DEPTH: usize = 20;

/// The objects two states' references point at, for settling whether two
/// references that differ by name point at the same thing.
pub(crate) struct Referents<'a> {
    before: &'a CadDatabase,
    after: &'a CadDatabase,
    tolerance: Tolerance,
    /// Pairs already compared: a block is often inserted many times, an
    /// image definition shown many times.
    seen: RefCell<BTreeMap<(Referent, String, String), bool>>,
}

impl<'a> Referents<'a> {
    pub(crate) fn new(
        before: &'a CadDatabase,
        after: &'a CadDatabase,
        tolerance: Tolerance,
    ) -> Self {
        Referents {
            before,
            after,
            tolerance,
            seen: RefCell::new(BTreeMap::new()),
        }
    }

    /// `fields`, the field changes of an entity whose JSON forms are
    /// `before` and `after`, without the change of a reference whose two
    /// objects hold the same ([`fields::by_content`]).
    pub(crate) fn settle(
        &self,
        before: &Value,
        after: &Value,
        fields: Vec<FieldChange>,
    ) -> Vec<FieldChange> {
        if fields::by_content_keys(before).next().is_none() {
            return fields;
        }
        self.settle_at(before, after, fields.clone(), 0)
            .unwrap_or(fields)
    }

    /// [`Self::settle`] at a nesting `depth`; `None` when a reference that
    /// changed points at objects that do not hold the same.
    fn settle_at(
        &self,
        before: &Value,
        after: &Value,
        mut fields: Vec<FieldChange>,
        depth: usize,
    ) -> Option<Vec<FieldChange>> {
        for (key, kind) in fields::by_content_keys(before) {
            let path = format!("{key}.data");
            if !fields.iter().any(|f| f.path == path) {
                continue;
            }
            let same = fields::by_content(before, key) == Some(kind)
                && fields::by_content(after, key) == Some(kind)
                && match (name(&before[key]), name(&after[key])) {
                    (Some(b), Some(a)) => self.same(kind, b, a, depth),
                    _ => false,
                };
            if !same {
                return None;
            }
            fields.retain(|f| f.path != path);
        }
        Some(fields)
    }

    /// Whether `b` of the first state and `a` of the second, objects of
    /// kind `kind`, hold the same.
    fn same(&self, kind: Referent, b: &str, a: &str, depth: usize) -> bool {
        if depth > MAX_DEPTH {
            return false;
        }
        let key = (kind, b.to_string(), a.to_string());
        if let Some(&known) = self.seen.borrow().get(&key) {
            return known;
        }
        let verdict = match kind {
            Referent::AnonymousBlock => self.same_block(b, a, depth),
            Referent::ImageDefinition => self.same_record(
                self.before.tables.image_definitions.get(b),
                self.after.tables.image_definitions.get(a),
            ),
        };
        self.seen.borrow_mut().insert(key, verdict);
        verdict
    }

    /// Two records of one kind: both present and no field beyond tolerance.
    fn same_record<T: serde::Serialize>(&self, b: Option<&T>, a: Option<&T>) -> bool {
        let (Some(b), Some(a)) = (b, a) else {
            return false;
        };
        within(&fields::compare(&json(b), &json(a), self.tolerance))
    }

    /// The same entities, in the same order, within tolerance -- and the
    /// same base point. An entity of one that refers by content may name
    /// another object than its counterpart, if those two hold the same in
    /// turn.
    fn same_block(&self, b: &str, a: &str, depth: usize) -> bool {
        let (Some(bb), Some(ab)) = (
            self.before.tables.block_records.get(b),
            self.after.tables.block_records.get(a),
        ) else {
            return false;
        };
        if bb.entities.len() != ab.entities.len()
            || !self.same_record(Some(&bb.base_point), Some(&ab.base_point))
        {
            return false;
        }
        bb.entities.iter().zip(&ab.entities).all(|(x, y)| {
            if x.type_name() != y.type_name() {
                return false;
            }
            let (xv, yv) = (json(x), json(y));
            let changes = fields::compare(&xv, &yv, self.tolerance);
            self.settle_at(&xv, &yv, changes, depth + 1)
                .is_some_and(|left| within(&left))
        })
    }
}

/// The name a resolved reference gives.
fn name(reference: &Value) -> Option<&str> {
    if reference["type"] != "RESOLVED" {
        return None;
    }
    reference["data"].as_str()
}

fn json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("the model serializes")
}

/// No field differs beyond tolerance.
fn within(changes: &[FieldChange]) -> bool {
    changes.iter().all(|f| f.verdict == Verdict::Within)
}
