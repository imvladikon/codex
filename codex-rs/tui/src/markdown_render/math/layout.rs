use super::MAX_MATH_COLUMNS;
use super::MAX_MATH_ROWS;
use super::RenderedMath;
use crate::width::char_width;
use crate::width::display_width;
use ratex_font::symbols;
use ratex_parser::Mode;
use ratex_parser::ParseNode;
use ratex_parser::parse_node::AtomFamily;

mod block;
mod scripts;

use block::Block;
use block::center;
use block::pad_right;
use scripts::render_scripts;

pub(super) fn render(nodes: &[ParseNode]) -> Option<RenderedMath> {
    let block = render_sequence(nodes)?;
    let rows = block
        .rows
        .into_iter()
        .map(|row| row.trim_end().to_owned())
        .collect::<Vec<_>>();
    let width = rows.iter().map(|row| display_width(row)).max().unwrap_or(0);
    if rows.is_empty()
        || rows.len() > MAX_MATH_ROWS
        || block.baseline >= rows.len()
        || width == 0
        || width > MAX_MATH_COLUMNS
        || rows.iter().any(|row| {
            row.contains('\\') || row.chars().any(|character| char_width(character) == 0)
        })
    {
        return None;
    }
    Some(RenderedMath {
        rows,
        width,
        baseline: block.baseline,
    })
}

fn render_sequence(nodes: &[ParseNode]) -> Option<Block> {
    nodes.iter().try_fold(Block::empty(), |block, node| {
        block.beside(&render_node(node)?)
    })
}

fn render_node(node: &ParseNode) -> Option<Block> {
    match node {
        ParseNode::Atom {
            mode, family, text, ..
        } => {
            let symbol = resolve_symbol(text, *mode)?;
            let symbol = match family {
                AtomFamily::Bin | AtomFamily::Rel => format!(" {symbol} "),
                AtomFamily::Punct => format!("{symbol} "),
                AtomFamily::Close | AtomFamily::Inner | AtomFamily::Open => symbol,
            };
            Block::text(symbol)
        }
        ParseNode::MathOrd { mode, text, .. }
        | ParseNode::TextOrd { mode, text, .. }
        | ParseNode::OpToken { mode, text, .. }
        | ParseNode::AccentToken { mode, text, .. } => Block::text(resolve_symbol(text, *mode)?),
        ParseNode::SpacingNode { .. } | ParseNode::Kern { .. } => Block::text(" "),
        ParseNode::OrdGroup { body, .. }
        | ParseNode::Text { body, .. }
        | ParseNode::Color { body, .. }
        | ParseNode::Styling { body, .. }
        | ParseNode::Sizing { body, .. }
        | ParseNode::MClass { body, .. }
        | ParseNode::Href { body, .. }
        | ParseNode::HBox { body, .. }
        | ParseNode::Pmb { body, .. }
        | ParseNode::Html { body, .. } => render_sequence(body),
        ParseNode::SupSub { base, sup, sub, .. } => render_scripts(
            render_optional(base.as_deref())?,
            render_optional(sup.as_deref())?,
            render_optional(sub.as_deref())?,
        ),
        ParseNode::GenFrac {
            numer,
            denom,
            has_bar_line,
            left_delim,
            right_delim,
            ..
        } => render_fraction(
            render_node(numer)?,
            render_node(denom)?,
            *has_bar_line,
            left_delim.as_deref(),
            right_delim.as_deref(),
        ),
        ParseNode::Sqrt { body, index, .. } => {
            render_sqrt(render_node(body)?, render_optional(index.as_deref())?)
        }
        ParseNode::Accent {
            mode, label, base, ..
        } => render_accent(
            render_node(base)?,
            resolve_symbol(label, *mode)?.as_str(),
            true,
        ),
        ParseNode::AccentUnder {
            mode, label, base, ..
        } => render_accent(
            render_node(base)?,
            resolve_symbol(label, *mode)?.as_str(),
            false,
        ),
        ParseNode::Op {
            mode, name, body, ..
        } => {
            if let Some(body) = body {
                render_sequence(body)
            } else {
                Block::text(resolve_operator(name.as_deref()?, *mode)?)
            }
        }
        ParseNode::OperatorName { body, .. } => render_sequence(body),
        ParseNode::Font { body, .. }
        | ParseNode::Smash { body, .. }
        | ParseNode::Lap { body, .. }
        | ParseNode::RaiseBox { body, .. }
        | ParseNode::VCenter { body, .. } => render_node(body),
        ParseNode::ColorToken { .. }
        | ParseNode::Size { .. }
        | ParseNode::Cr { .. }
        | ParseNode::Internal { .. }
        | ParseNode::NoNumber { .. } => Some(Block::empty()),
        ParseNode::DelimSizing { mode, delim, .. }
        | ParseNode::LeftRightRight { mode, delim, .. }
        | ParseNode::Middle { mode, delim, .. } => Block::text(resolve_symbol(delim, *mode)?),
        ParseNode::LeftRight {
            mode,
            body,
            left,
            right,
            ..
        } => wrap_delimiters(
            render_sequence(body)?,
            &resolve_delimiter(left, *mode)?,
            &resolve_delimiter(right, *mode)?,
        ),
        ParseNode::Overline { body, .. } => decorate_line(render_node(body)?, true),
        ParseNode::Underline { body, .. } => decorate_line(render_node(body)?, false),
        ParseNode::Rule { .. }
        | ParseNode::Environment { .. }
        | ParseNode::CdArrow { .. }
        | ParseNode::ProofTree { .. } => None,
        ParseNode::Phantom { body, .. } => blank_like(render_sequence(body)?),
        ParseNode::VPhantom { body, .. } => blank_like(render_node(body)?),
        ParseNode::Array { body, .. } => render_array(body),
        ParseNode::Infix { replace_with, .. } => {
            Block::text(resolve_operator(replace_with, Mode::Math)?)
        }
        ParseNode::Verb { body, .. } => Block::text(body),
        ParseNode::Url { url, .. } => Block::text(url),
        ParseNode::Raw { string, .. } => Block::text(string),
        ParseNode::HorizBrace {
            label,
            is_over,
            base,
            ..
        } => render_accent(render_node(base)?, brace_glyph(label), *is_over),
        ParseNode::Enclose { label, body, .. } if label == r"\fbox" => boxed(render_node(body)?),
        ParseNode::Enclose { .. } => None,
        ParseNode::MathChoice { display, .. } => render_sequence(display),
        ParseNode::XArrow {
            label, body, below, ..
        } => render_arrow(
            label,
            render_node(body)?,
            render_optional(below.as_deref())?,
        ),
        ParseNode::Tag { body, tag, .. } => render_sequence(body)?
            .beside(&Block::text("  (")?)?
            .beside(&render_sequence(tag)?)?
            .beside(&Block::text(")")?),
        ParseNode::HtmlMathMl { mathml, .. } => render_sequence(mathml),
        ParseNode::IncludeGraphics { alt, .. } => Block::text(alt),
        ParseNode::CdLabel { label, .. } => render_node(label),
        ParseNode::CdLabelParent { fragment, .. } => render_node(fragment),
    }
}

fn render_optional(node: Option<&ParseNode>) -> Option<Option<Block>> {
    match node {
        Some(node) => Some(Some(render_node(node)?)),
        None => Some(None),
    }
}

fn resolve_symbol(text: &str, mode: Mode) -> Option<String> {
    if text == "." {
        return Some(String::new());
    }
    let font_mode = match mode {
        Mode::Math => symbols::Mode::Math,
        Mode::Text => symbols::Mode::Text,
    };
    if let Some(symbol) = symbols::get_symbol(text, font_mode) {
        if symbol.group == symbols::Group::Spacing {
            return Some(" ".to_string());
        }
        if let Some(codepoint) = symbol.codepoint {
            return Some(codepoint.to_string());
        }
    }
    (!text.starts_with('\\')).then(|| text.to_owned())
}

fn resolve_operator(name: &str, mode: Mode) -> Option<String> {
    resolve_symbol(name, mode).or_else(|| {
        name.strip_prefix('\\')
            .filter(|name| name.chars().all(char::is_alphabetic))
            .map(str::to_owned)
    })
}

fn resolve_delimiter(delimiter: &str, mode: Mode) -> Option<String> {
    (delimiter == ".")
        .then(String::new)
        .or_else(|| resolve_symbol(delimiter, mode))
}

fn render_fraction(
    numerator: Block,
    denominator: Block,
    has_bar_line: bool,
    left_delimiter: Option<&str>,
    right_delimiter: Option<&str>,
) -> Option<Block> {
    let width = numerator.width().max(denominator.width()).max(1);
    let mut rows = numerator
        .rows
        .iter()
        .map(|row| center(row, width))
        .collect::<Vec<_>>();
    let baseline = rows.len();
    if has_bar_line {
        rows.push("─".repeat(width));
    }
    rows.extend(denominator.rows.iter().map(|row| center(row, width)));
    let block = Block::new(rows, baseline)?;
    let left = match left_delimiter {
        Some(delimiter) => resolve_delimiter(delimiter, Mode::Math)?,
        None => String::new(),
    };
    let right = match right_delimiter {
        Some(delimiter) => resolve_delimiter(delimiter, Mode::Math)?,
        None => String::new(),
    };
    wrap_delimiters(block, &left, &right)
}

fn render_sqrt(body: Block, index: Option<Block>) -> Option<Block> {
    let index = match index {
        Some(index) if index.rows.len() == 1 => index.rows[0].clone(),
        Some(_) => return None,
        None => String::new(),
    };
    let prefix_width = display_width(&index) + 2;
    let mut rows = vec![format!(
        "{}{}",
        " ".repeat(prefix_width),
        "─".repeat(body.width())
    )];
    for (row_index, row) in body.rows.iter().enumerate() {
        let prefix = if row_index == body.baseline {
            format!("{index}√ ")
        } else {
            format!("{}│ ", " ".repeat(display_width(&index)))
        };
        rows.push(format!("{prefix}{}", pad_right(row, body.width())));
    }
    Block::new(rows, body.baseline + 1)
}

fn render_accent(body: Block, accent: &str, over: bool) -> Option<Block> {
    let accent_row = center(accent, body.width());
    let mut rows = body.rows;
    let baseline = if over {
        rows.insert(0, accent_row);
        body.baseline + 1
    } else {
        rows.push(accent_row);
        body.baseline
    };
    Block::new(rows, baseline)
}

fn decorate_line(body: Block, over: bool) -> Option<Block> {
    let line = "─".repeat(body.width());
    render_accent(body, &line, over)
}

fn brace_glyph(label: &str) -> &str {
    if label.contains("under") {
        "⏟"
    } else {
        "⏞"
    }
}

fn boxed(body: Block) -> Option<Block> {
    let width = body.width();
    let mut rows = Vec::with_capacity(body.rows.len() + 2);
    rows.push(format!("┌{}┐", "─".repeat(width)));
    rows.extend(
        body.rows
            .iter()
            .map(|row| format!("│{}│", pad_right(row, width))),
    );
    rows.push(format!("└{}┘", "─".repeat(width)));
    Block::new(rows, body.baseline + 1)
}

fn blank_like(body: Block) -> Option<Block> {
    Block::new(
        body.rows
            .iter()
            .map(|row| " ".repeat(display_width(row)))
            .collect(),
        body.baseline,
    )
}

fn wrap_delimiters(body: Block, left: &str, right: &str) -> Option<Block> {
    if left.is_empty() && right.is_empty() {
        return Some(body);
    }
    let height = body.rows.len();
    let width = body.width();
    let rows = body
        .rows
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            format!(
                "{}{}{}",
                delimiter_piece(left, row_index, height, body.baseline),
                pad_right(row, width),
                delimiter_piece(right, row_index, height, body.baseline)
            )
        })
        .collect();
    Block::new(rows, body.baseline)
}

fn delimiter_piece(delimiter: &str, row: usize, height: usize, baseline: usize) -> String {
    if delimiter.is_empty() {
        return String::new();
    }
    let Some(character) = delimiter.chars().next() else {
        return String::new();
    };
    if height == 1 {
        return character.to_string();
    }
    let character = match character {
        '(' if row == 0 => '⎛',
        '(' if row + 1 == height => '⎝',
        '(' => '⎜',
        ')' if row == 0 => '⎞',
        ')' if row + 1 == height => '⎠',
        ')' => '⎟',
        '[' if row == 0 => '⎡',
        '[' if row + 1 == height => '⎣',
        '[' => '⎢',
        ']' if row == 0 => '⎤',
        ']' if row + 1 == height => '⎦',
        ']' => '⎥',
        '{' if row == 0 => '⎧',
        '{' if row + 1 == height => '⎩',
        '{' if row == height / 2 => '⎨',
        '{' => '⎪',
        '}' if row == 0 => '⎫',
        '}' if row + 1 == height => '⎭',
        '}' if row == height / 2 => '⎬',
        '}' => '⎪',
        '|' => '│',
        '‖' => '‖',
        character if row == baseline => character,
        _ => ' ',
    };
    character.to_string()
}

fn render_array(rows: &[Vec<ParseNode>]) -> Option<Block> {
    if rows.is_empty() {
        return Some(Block::empty());
    }
    let cells = rows
        .iter()
        .map(|row| row.iter().map(render_node).collect::<Option<Vec<_>>>())
        .collect::<Option<Vec<_>>>()?;
    let columns = cells.iter().map(Vec::len).max().unwrap_or(0);
    let column_widths = (0..columns)
        .map(|column| {
            cells
                .iter()
                .filter_map(|row| row.get(column))
                .map(Block::width)
                .max()
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    let mut output = Vec::new();
    let center_row = cells.len() / 2;
    let mut baseline = None;
    for (row_index, row) in cells.iter().enumerate() {
        let row_baseline = row.iter().map(|cell| cell.baseline).max().unwrap_or(0);
        let row_height = row
            .iter()
            .map(|cell| row_baseline - cell.baseline + cell.rows.len())
            .max()
            .unwrap_or(1);
        if row_index == center_row {
            baseline = Some(output.len() + row_baseline);
        }
        for line_index in 0..row_height {
            let mut line = String::new();
            for (column, &column_width) in column_widths.iter().enumerate() {
                if column > 0 {
                    line.push_str("  ");
                }
                let cell = row.get(column);
                let cell_row = cell
                    .and_then(|cell| {
                        line_index
                            .checked_sub(row_baseline - cell.baseline)
                            .and_then(|index| cell.rows.get(index))
                    })
                    .map(String::as_str)
                    .unwrap_or_default();
                line.push_str(&center(cell_row, column_width));
            }
            output.push(line);
        }
    }
    Block::new(output, baseline.unwrap_or_default())
}

fn render_arrow(label: &str, above: Block, below: Option<Block>) -> Option<Block> {
    let arrow = if label.contains("left") { '←' } else { '→' };
    let width = above
        .width()
        .max(below.as_ref().map_or(0, Block::width))
        .max(3);
    let mut rows = above
        .rows
        .iter()
        .map(|row| center(row, width))
        .collect::<Vec<_>>();
    let baseline = rows.len();
    rows.push(format!("{}{}", "─".repeat(width - 1), arrow));
    if let Some(below) = below {
        rows.extend(below.rows.iter().map(|row| center(row, width)));
    }
    Block::new(rows, baseline)
}
