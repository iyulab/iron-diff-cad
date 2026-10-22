//! Text fields whose strings differ only in how they are formatted.
//!
//! A drawing saved again may write a text's formatting codes differently
//! -- an MTEXT paragraph alignment it states explicitly where the older
//! save left the default -- while the text reads the same. Which part of a
//! string is formatting is the model's to say ([`uncad_model::text::tokens`]);
//! nothing here reads the codes itself.

use crate::change_set::{Change, ChangeSet};
use uncad_model::text::{tokens, TextKind, Token};

/// The text codes the field at `path` of an entity of `entity_type` is
/// written in, when it holds a text at all.
fn kind(entity_type: &str, path: &str) -> Option<TextKind> {
    match (entity_type, path) {
        ("TEXT" | "ATTRIB", "text") | ("ATTDEF", "default_value") => Some(TextKind::Line),
        ("MTEXT", "text")
        | ("TOLERANCE", "text_value")
        | ("DIMENSION", "text_override.data")
        | ("MULTILEADER", "content.data.text") => Some(TextKind::MText),
        ("INSERT", p) if p.starts_with("attribs[") && p.ends_with("].text") => Some(TextKind::Line),
        _ => None,
    }
}

/// `text` without its formatting: every token but property switches,
/// toggles and groups.
fn content(text: &str, kind: TextKind) -> Vec<Token<'_>> {
    tokens(text, kind)
        .filter(|t| {
            !matches!(
                t,
                Token::Property { .. } | Token::Toggle(_) | Token::GroupStart | Token::GroupEnd
            )
        })
        .collect()
}

/// Marks every text field change of a `MODIFIED` entry whose two strings
/// read the same ([`crate::FieldChange::format_only`]).
pub(crate) fn mark_format_only(set: &mut ChangeSet) {
    for change in &mut set.changes {
        let Change::Modified(m) = change else {
            continue;
        };
        for field in &mut m.fields {
            let (Some(kind), Some(before), Some(after)) = (
                kind(&m.entity_type, &field.path),
                field.before.as_str(),
                field.after.as_str(),
            ) else {
                continue;
            };
            field.format_only = content(before, kind) == content(after, kind);
        }
    }
}
