use super::HyperlinkLine;
use super::Writer;
use crate::width::char_width;
use crate::width::display_width;
use pulldown_cmark::CowStr;
use pulldown_cmark::Event;
use ratatui::text::Line;
use ratatui::text::Span;
use std::ops::Range;

mod layout;
mod normalization;

pub(super) use normalization::LiteralDollarEncoding;
pub(super) use normalization::normalize_tex_delimiters;
pub(super) use normalization::restore_literal_dollars;

const MAX_MATH_SOURCE_BYTES: usize = 8 * 1024;
const MAX_MATH_ROWS: usize = 24;
const MAX_MATH_COLUMNS: usize = 256;
const LITERAL_DOLLAR_SENTINELS: [u8; 4] = [0x1e, 0x1f, 0x1d, 0x1c];

pub(super) struct RenderedMath {
    pub(super) rows: Vec<String>,
    pub(super) width: usize,
    baseline: usize,
}

impl<'a, 'policy, I> Writer<'a, 'policy, I>
where
    I: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    pub(super) fn inline_math(&mut self, source: CowStr<'a>, range: Range<usize>) {
        if self.suppressing_local_link_label() {
            return;
        }
        self.line_ends_with_local_link_target = false;
        let original = self.input.get(range.clone()).unwrap_or_default();
        let source = source_body(&source, original);
        if original.starts_with('$') && looks_like_literal_dollars(self.input, source, &range) {
            self.text(self.raw_math(source, range, /*display*/ false).into());
            return;
        }
        let rendered = render(source).filter(|rendered| {
            self.available_math_width()
                .is_none_or(|width| rendered.width <= width)
        });
        let Some(row) = rendered.as_ref().and_then(inline_row) else {
            self.text(self.raw_math(source, range, /*display*/ false).into());
            return;
        };
        if self
            .available_math_width()
            .is_some_and(|width| display_width(&row) > width)
        {
            self.text(self.raw_math(source, range, /*display*/ false).into());
            return;
        }
        let style = self.inline_styles.last().copied().unwrap_or_default();
        if self.in_table_cell() {
            self.push_text_spans_to_table_cell(&row, style);
        } else {
            if self.pending_marker_line {
                self.push_line(Line::default());
            }
            self.pending_marker_line = false;
            self.push_text_spans(&row, style);
        }
    }

    pub(super) fn display_math(&mut self, source: CowStr<'a>, range: Range<usize>) {
        if self.suppressing_local_link_label() {
            return;
        }
        self.line_ends_with_local_link_target = false;
        let original = self.input.get(range.clone()).unwrap_or_default();
        let source = source_body(&source, original);
        if original.len() >= 4 && original.bytes().all(|byte| byte == b'$') {
            self.text(original.to_owned().into());
            return;
        }
        if self.in_table_cell() {
            let raw = self.raw_math(source, range, /*display*/ true);
            let style = self.inline_styles.last().copied().unwrap_or_default();
            self.push_text_spans_to_table_cell(&raw, style);
            return;
        }
        if self.link.is_some() {
            let raw = self.raw_math(source, range, /*display*/ true);
            self.text(raw.into());
            return;
        }

        let rendered = render(source).filter(|rendered| {
            self.available_math_width()
                .is_none_or(|width| rendered.width <= width)
        });
        let Some(rendered) = rendered else {
            if self.current_line_has_content() {
                self.flush_current_line();
            }
            self.text(self.raw_math(source, range, /*display*/ true).into());
            self.needs_newline = true;
            return;
        };

        let style = self.inline_styles.last().copied().unwrap_or_default();
        let mut rows = rendered.rows.into_iter();
        let Some(first_row) = rows.next() else {
            return;
        };
        if self.current_line_is_empty() {
            if let Some(line) = self.current_line_content.as_mut() {
                line.line.push_span(Span::styled(first_row, style));
            }
            self.current_line_skip_wrap = true;
            self.flush_current_line();
        } else {
            self.flush_current_line();
            self.push_line(Line::from(Span::styled(first_row, style)));
            self.current_line_skip_wrap = true;
            self.flush_current_line();
        }
        for row in rows {
            self.push_prewrapped_line(
                HyperlinkLine::new(Line::from(Span::styled(row, style))),
                /*pending_marker_line*/ false,
            );
        }
        self.needs_newline = true;
    }

    fn raw_math(&self, source: &str, range: Range<usize>, display: bool) -> String {
        let original = self.input.get(range).unwrap_or_default();
        if (display && (original.starts_with("$$") || original.starts_with(r"\[")))
            || (!display && (original.starts_with('$') || original.starts_with(r"\(")))
        {
            return original.to_owned();
        }
        if display {
            format!("$${source}$$")
        } else {
            format!("${source}$")
        }
    }

    fn current_line_has_content(&self) -> bool {
        self.current_line_content
            .as_ref()
            .is_some_and(|line| line.line.spans.iter().any(|span| !span.content.is_empty()))
    }

    fn current_line_is_empty(&self) -> bool {
        self.current_line_content.is_some() && !self.current_line_has_content()
    }

    fn available_math_width(&self) -> Option<usize> {
        self.wrap_width.map(|wrap_width| {
            let prefix_width = if self.current_line_content.is_some() {
                Self::spans_display_width(&self.current_initial_indent)
            } else {
                Self::spans_display_width(&self.prefix_spans(self.pending_marker_line))
            };
            wrap_width.saturating_sub(prefix_width)
        })
    }
}

pub(super) fn looks_like_literal_dollars(input: &str, source: &str, range: &Range<usize>) -> bool {
    let suffix = input.get(range.end..).unwrap_or_default();
    let currency = source
        .strip_suffix(['-', '–', '—', '+', '/', ':', ';', '='])
        .is_some_and(|amount| {
            amount.starts_with(char::is_numeric)
                && amount.ends_with(char::is_numeric)
                && amount
                    .chars()
                    .all(|character| character.is_ascii_digit() || matches!(character, '.' | ','))
        })
        && suffix
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_digit());
    let shell_variable = source
        .strip_suffix([':', '/', ';', '-', '=', '.', '+'])
        .is_some_and(is_shell_variable)
        && starts_with_shell_variable(suffix);
    currency || shell_variable
}

fn source_body<'a>(source: &'a str, original: &str) -> &'a str {
    if original.starts_with(r"\(") && original.ends_with(r"\)") {
        source
            .strip_prefix('{')
            .and_then(|source| source.strip_suffix('}'))
            .unwrap_or(source)
    } else {
        source
    }
}

fn is_shell_variable(source: &str) -> bool {
    let source = source
        .strip_prefix('{')
        .and_then(|source| source.strip_suffix('}'))
        .unwrap_or(source);
    let mut characters = source.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn starts_with_shell_variable(source: &str) -> bool {
    split_shell_variable(source).is_some()
}

fn split_shell_variable(source: &str) -> Option<(&str, &str)> {
    if let Some(source) = source.strip_prefix('{') {
        return source
            .split_once('}')
            .filter(|(variable, _)| is_shell_variable(variable));
    }
    let variable_len = source
        .char_indices()
        .take_while(|(_, character)| *character == '_' || character.is_ascii_alphanumeric())
        .map(|(offset, character)| offset + character.len_utf8())
        .last()?;
    let (variable, remainder) = source.split_at(variable_len);
    is_shell_variable(variable).then_some((variable, remainder))
}

pub(super) fn render(source: &str) -> Option<RenderedMath> {
    if source.len() > MAX_MATH_SOURCE_BYTES
        || source.as_bytes().contains(&b'$')
        || source
            .chars()
            .any(|character| !matches!(character, '\r' | '\n') && char_width(character) == 0)
        || [r"\(", r"\)", r"\[", r"\]"]
            .into_iter()
            .any(|delimiter| source.contains(delimiter))
    {
        return None;
    }

    let nodes = ratex_parser::parse(source.trim()).ok()?;
    layout::render(&nodes)
}

fn inline_row(rendered: &RenderedMath) -> Option<String> {
    if rendered.rows.len() == 1 {
        return rendered.rows.first().cloned();
    }
    if rendered.baseline >= rendered.rows.len()
        || rendered.rows.iter().any(|row| {
            row.contains('\\')
                || row.chars().any(|character| char_width(character) != 1)
                || row.chars().any(|character| {
                    matches!(
                        character,
                        '─' | '━'
                            | '═'
                            | '│'
                            | '⎮'
                            | '⌠'
                            | '⌡'
                            | '√'
                            | '⎛'
                            | '⎜'
                            | '⎝'
                            | '⎞'
                            | '⎟'
                            | '⎠'
                    )
                })
        })
    {
        return None;
    }

    let rows = rendered
        .rows
        .iter()
        .map(|row| row.chars().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut output = String::new();
    let mut active_script = None;
    for column in 0..columns {
        let baseline = rows[rendered.baseline]
            .get(column)
            .copied()
            .filter(|character| !character.is_whitespace());
        let superscript = rows[..rendered.baseline]
            .iter()
            .filter_map(|row| row.get(column).copied())
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        let subscript = rows[rendered.baseline + 1..]
            .iter()
            .filter_map(|row| row.get(column).copied())
            .filter(|character| !character.is_whitespace())
            .collect::<String>();

        if let Some(character) = baseline {
            close_script(&mut output, &mut active_script);
            output.push(character);
        }
        append_script(
            &mut output,
            &mut active_script,
            ScriptPosition::Above,
            &superscript,
        );
        append_script(
            &mut output,
            &mut active_script,
            ScriptPosition::Below,
            &subscript,
        );
        if baseline.is_none() && superscript.is_empty() && subscript.is_empty() {
            close_script(&mut output, &mut active_script);
            if !output.ends_with(' ') {
                output.push(' ');
            }
        }
    }
    close_script(&mut output, &mut active_script);
    let output = output.trim().to_owned();
    (!output.is_empty()).then_some(output)
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ScriptPosition {
    Above,
    Below,
}

fn append_script(
    output: &mut String,
    active_script: &mut Option<ScriptPosition>,
    position: ScriptPosition,
    text: &str,
) {
    if text.is_empty() {
        return;
    }
    if *active_script != Some(position) {
        close_script(output, active_script);
        output.push(match position {
            ScriptPosition::Above => '⁽',
            ScriptPosition::Below => '₍',
        });
        *active_script = Some(position);
    }
    output.push_str(text);
}

fn close_script(output: &mut String, active_script: &mut Option<ScriptPosition>) {
    if let Some(position) = active_script.take() {
        output.push(match position {
            ScriptPosition::Above => '⁾',
            ScriptPosition::Below => '₎',
        });
    }
}

#[cfg(test)]
#[path = "math_tests.rs"]
mod tests;
