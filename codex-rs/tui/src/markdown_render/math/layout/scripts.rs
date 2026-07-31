use super::super::MAX_MATH_COLUMNS;
use super::block::Block;
use super::block::pad_right;

pub(super) fn render_scripts(
    base: Option<Block>,
    sup: Option<Block>,
    sub: Option<Block>,
) -> Option<Block> {
    let base = base.unwrap_or_else(Block::empty);
    if base.rows.len() == 1
        && sup.as_ref().is_none_or(|block| block.rows.len() == 1)
        && sub.as_ref().is_none_or(|block| block.rows.len() == 1)
    {
        let superscript = match sup.as_ref() {
            Some(block) => script_text(&block.rows[0], ScriptKind::Superscript),
            None => Some(String::new()),
        };
        let subscript = match sub.as_ref() {
            Some(block) => script_text(&block.rows[0], ScriptKind::Subscript),
            None => Some(String::new()),
        };
        if let (Some(superscript), Some(subscript)) = (superscript, subscript) {
            return Block::text(format!("{}{subscript}{superscript}", base.rows[0]));
        }
    }
    let script_width = sup
        .as_ref()
        .into_iter()
        .chain(sub.as_ref())
        .map(Block::width)
        .max()
        .unwrap_or(0);
    if base
        .width()
        .checked_add(script_width)
        .is_none_or(|width| width > MAX_MATH_COLUMNS)
    {
        return None;
    }
    let sup_height = sup.as_ref().map_or(0, |block| block.rows.len());
    let sub_height = sub.as_ref().map_or(0, |block| block.rows.len());
    let mut rows = Vec::with_capacity(sup_height + base.rows.len() + sub_height);
    if let Some(sup) = sup {
        rows.extend(sup.rows.iter().map(|row| {
            format!(
                "{}{}",
                " ".repeat(base.width()),
                pad_right(row, script_width)
            )
        }));
    }
    rows.extend(base.rows.iter().map(|row| {
        format!(
            "{}{}",
            pad_right(row, base.width()),
            " ".repeat(script_width)
        )
    }));
    if let Some(sub) = sub {
        rows.extend(sub.rows.iter().map(|row| {
            format!(
                "{}{}",
                " ".repeat(base.width()),
                pad_right(row, script_width)
            )
        }));
    }
    Block::new(rows, sup_height + base.baseline)
}

#[derive(Clone, Copy)]
enum ScriptKind {
    Superscript,
    Subscript,
}

fn script_text(text: &str, kind: ScriptKind) -> Option<String> {
    text.chars()
        .map(|character| {
            Some(match (kind, character) {
                (ScriptKind::Superscript, '0') => '⁰',
                (ScriptKind::Superscript, '1') => '¹',
                (ScriptKind::Superscript, '2') => '²',
                (ScriptKind::Superscript, '3') => '³',
                (ScriptKind::Superscript, '4') => '⁴',
                (ScriptKind::Superscript, '5') => '⁵',
                (ScriptKind::Superscript, '6') => '⁶',
                (ScriptKind::Superscript, '7') => '⁷',
                (ScriptKind::Superscript, '8') => '⁸',
                (ScriptKind::Superscript, '9') => '⁹',
                (ScriptKind::Superscript, '+') => '⁺',
                (ScriptKind::Superscript, '-') => '⁻',
                (ScriptKind::Superscript, '=') => '⁼',
                (ScriptKind::Superscript, '(') => '⁽',
                (ScriptKind::Superscript, ')') => '⁾',
                (ScriptKind::Superscript, 'n') => 'ⁿ',
                (ScriptKind::Superscript, 'i') => 'ⁱ',
                (ScriptKind::Subscript, '0') => '₀',
                (ScriptKind::Subscript, '1') => '₁',
                (ScriptKind::Subscript, '2') => '₂',
                (ScriptKind::Subscript, '3') => '₃',
                (ScriptKind::Subscript, '4') => '₄',
                (ScriptKind::Subscript, '5') => '₅',
                (ScriptKind::Subscript, '6') => '₆',
                (ScriptKind::Subscript, '7') => '₇',
                (ScriptKind::Subscript, '8') => '₈',
                (ScriptKind::Subscript, '9') => '₉',
                (ScriptKind::Subscript, '+') => '₊',
                (ScriptKind::Subscript, '-') => '₋',
                (ScriptKind::Subscript, '=') => '₌',
                (ScriptKind::Subscript, '(') => '₍',
                (ScriptKind::Subscript, ')') => '₎',
                (ScriptKind::Subscript, 'a') => 'ₐ',
                (ScriptKind::Subscript, 'e') => 'ₑ',
                (ScriptKind::Subscript, 'h') => 'ₕ',
                (ScriptKind::Subscript, 'i') => 'ᵢ',
                (ScriptKind::Subscript, 'j') => 'ⱼ',
                (ScriptKind::Subscript, 'k') => 'ₖ',
                (ScriptKind::Subscript, 'l') => 'ₗ',
                (ScriptKind::Subscript, 'm') => 'ₘ',
                (ScriptKind::Subscript, 'n') => 'ₙ',
                (ScriptKind::Subscript, 'o') => 'ₒ',
                (ScriptKind::Subscript, 'p') => 'ₚ',
                (ScriptKind::Subscript, 'r') => 'ᵣ',
                (ScriptKind::Subscript, 's') => 'ₛ',
                (ScriptKind::Subscript, 't') => 'ₜ',
                (ScriptKind::Subscript, 'u') => 'ᵤ',
                (ScriptKind::Subscript, 'v') => 'ᵥ',
                (ScriptKind::Subscript, 'x') => 'ₓ',
                _ => return None,
            })
        })
        .collect()
}
