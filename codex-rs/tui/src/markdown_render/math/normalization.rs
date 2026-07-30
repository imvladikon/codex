use super::super::markdown_options;
use super::LITERAL_DOLLAR_SENTINEL;
use super::looks_like_literal_dollars;
use pulldown_cmark::CowStr;
use pulldown_cmark::Event;
use pulldown_cmark::Options;
use pulldown_cmark::Parser;
use pulldown_cmark::Tag;
use pulldown_cmark::TagEnd;
use std::borrow::Cow;
use std::ops::Range;

pub(in crate::markdown_render) struct NormalizedMath<'a> {
    pub(in crate::markdown_render) source: Cow<'a, str>,
    pub(in crate::markdown_render) unclosed_math_start: Option<usize>,
}

struct DelimiterPair {
    open_offset: usize,
    close_offset: usize,
}

struct NativeMathRanges {
    literal_dollars: Vec<usize>,
    literal_math: Vec<Range<usize>>,
    unclosed_candidates: Vec<usize>,
}

struct MarkdownRanges {
    text: Vec<Range<usize>>,
    containers: Vec<Range<usize>>,
    math_regions: Vec<Range<usize>>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum MathRegionKind {
    Primary,
    ItemFallback,
}

struct MathRegionFrame {
    kind: MathRegionKind,
    range: Range<usize>,
}

pub(in crate::markdown_render) fn restore_literal_dollars(text: CowStr<'_>) -> CowStr<'_> {
    if text.as_bytes().contains(&LITERAL_DOLLAR_SENTINEL) {
        text.replace(LITERAL_DOLLAR_SENTINEL as char, "$").into()
    } else {
        text
    }
}

pub(in crate::markdown_render) fn normalize_tex_delimiters(input: &str) -> NormalizedMath<'_> {
    if !input.as_bytes().contains(&b'$') && !input.contains(r"\(") && !input.contains(r"\[") {
        return NormalizedMath {
            source: Cow::Borrowed(input),
            unclosed_math_start: None,
        };
    }

    let markdown = markdown_ranges(input);
    let mut native_math = native_math_ranges(input, &markdown.containers);
    native_math
        .literal_dollars
        .retain(|&offset| is_text_source(offset..offset + 1, &markdown.text));
    native_math
        .unclosed_candidates
        .retain(|&offset| is_text_source(offset..offset + 1, &markdown.text));
    let mut pairs = Vec::new();
    let full_input = 0..input.len();
    let (display_pairs, unclosed_display) = collect_paired_delimiters(
        input,
        &full_input,
        &markdown.text,
        &markdown.containers,
        b'[',
        b']',
    );
    pairs.extend(display_pairs.into_iter().map(|pair| (pair, b'[', b']')));
    let mut unclosed_math_start = unclosed_display;
    for region in markdown.math_regions {
        let (inline_pairs, unclosed_inline) = collect_paired_delimiters(
            input,
            &region,
            &markdown.text,
            &markdown.containers,
            b'(',
            b')',
        );
        pairs.extend(inline_pairs.into_iter().map(|pair| (pair, b'(', b')')));

        if input[region.end..].trim().is_empty() {
            unclosed_math_start = unclosed_math_start
                .into_iter()
                .chain(unclosed_inline)
                .chain(unclosed_native_math_start(
                    &region,
                    &native_math.unclosed_candidates,
                ))
                .min();
        }
    }
    pairs.sort_by_key(|(pair, _, _)| pair.open_offset);
    let mut previous_end = None;
    pairs.retain(|(pair, _, _)| {
        if previous_end.is_some_and(|end| pair.open_offset < end) {
            false
        } else {
            previous_end = Some(pair.close_offset + 2);
            true
        }
    });

    if pairs.is_empty()
        && native_math.literal_dollars.is_empty()
        && native_math.literal_math.is_empty()
    {
        return NormalizedMath {
            source: Cow::Borrowed(input),
            unclosed_math_start,
        };
    }

    let mut normalized = input.as_bytes().to_vec();
    for offset in native_math.literal_dollars {
        normalized[offset] = LITERAL_DOLLAR_SENTINEL;
    }
    for range in native_math.literal_math {
        normalized[range.start] = LITERAL_DOLLAR_SENTINEL;
        let close_offset = range.end.saturating_sub(1);
        if is_text_source(close_offset..range.end, &markdown.text) {
            normalized[close_offset] = LITERAL_DOLLAR_SENTINEL;
        }
    }
    for (pair, open, close) in pairs {
        match (open, close) {
            (b'[', b']') => {
                normalize_multiline_math_whitespace(
                    &mut normalized,
                    pair.open_offset + 2,
                    pair.close_offset,
                    &markdown.text,
                );
                normalized[pair.open_offset..pair.open_offset + 2].copy_from_slice(b"$$");
                normalized[pair.close_offset..pair.close_offset + 2].copy_from_slice(b"$$");
            }
            (b'(', b')') => {
                normalize_multiline_math_whitespace(
                    &mut normalized,
                    pair.open_offset + 2,
                    pair.close_offset,
                    &markdown.text,
                );
                normalized[pair.open_offset..pair.open_offset + 2].copy_from_slice(b"${");
                normalized[pair.close_offset..pair.close_offset + 2].copy_from_slice(b"}$");
            }
            _ => unreachable!("delimiter kinds are fixed above"),
        }
    }
    let source = String::from_utf8(normalized)
        .map(Cow::Owned)
        .unwrap_or_else(|_| Cow::Borrowed(input));
    NormalizedMath {
        source,
        unclosed_math_start,
    }
}

fn markdown_ranges(input: &str) -> MarkdownRanges {
    let mut text_ranges = Vec::new();
    let mut containers = Vec::new();
    let mut math_regions = Vec::new();
    let mut region_stack = Vec::new();
    let mut protected_depth = 0usize;
    let mut options = markdown_options();
    options.remove(Options::ENABLE_MATH);
    let parser = Parser::new_ext(input, options);
    containers.extend(
        parser
            .reference_definitions()
            .iter()
            .map(|(_, definition)| definition.span.clone()),
    );
    for (event, range) in parser.into_offset_iter() {
        match event {
            Event::Start(tag) => match tag {
                Tag::CodeBlock(_) | Tag::HtmlBlock | Tag::Image { .. } | Tag::MetadataBlock(_) => {
                    protected_depth += 1;
                    containers.push(range);
                }
                Tag::Link { .. } => containers.push(range),
                Tag::BlockQuote(_) => containers.push(range),
                Tag::Paragraph => region_stack.push(MathRegionFrame {
                    kind: MathRegionKind::Primary,
                    range,
                }),
                Tag::Heading { .. } => {
                    if starts_with_atx_heading(input, &range)
                        || ends_with_long_setext_underline(input, &range)
                    {
                        containers.push(range.clone());
                    }
                    region_stack.push(MathRegionFrame {
                        kind: MathRegionKind::Primary,
                        range,
                    });
                }
                Tag::TableCell => {
                    containers.push(range.clone());
                    region_stack.push(MathRegionFrame {
                        kind: MathRegionKind::Primary,
                        range,
                    });
                }
                Tag::Item => {
                    containers.push(range.clone());
                    region_stack.push(MathRegionFrame {
                        kind: MathRegionKind::ItemFallback,
                        range,
                    });
                }
                _ => {}
            },
            Event::End(
                TagEnd::CodeBlock | TagEnd::HtmlBlock | TagEnd::Image | TagEnd::MetadataBlock(_),
            ) => protected_depth = protected_depth.saturating_sub(1),
            Event::End(
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::TableCell | TagEnd::Item,
            ) => {
                region_stack.pop();
            }
            Event::Text(_) if protected_depth == 0 => {
                text_ranges.push(range);
                if let Some(frame) = region_stack
                    .iter()
                    .rev()
                    .find(|frame| frame.kind == MathRegionKind::Primary)
                    .or_else(|| region_stack.last())
                {
                    math_regions.push(frame.range.clone());
                }
            }
            Event::Code(_)
            | Event::Html(_)
            | Event::InlineHtml(_)
            | Event::FootnoteReference(_)
            | Event::Rule => {
                containers.push(range);
            }
            _ => {}
        }
    }
    math_regions.sort_by_key(|range| (range.start, range.end));
    math_regions.dedup();
    MarkdownRanges {
        text: text_ranges,
        containers,
        math_regions,
    }
}

fn starts_with_atx_heading(input: &str, range: &Range<usize>) -> bool {
    input[range.clone()]
        .trim_start_matches([' ', '\t'])
        .starts_with('#')
}

fn ends_with_long_setext_underline(input: &str, range: &Range<usize>) -> bool {
    let Some(underline) = input[range.clone()]
        .trim_end_matches(['\r', '\n'])
        .rsplit('\n')
        .next()
        .map(str::trim)
    else {
        return false;
    };
    underline.len() > 1
        && (underline.bytes().all(|byte| byte == b'=')
            || underline.bytes().all(|byte| byte == b'-'))
}

fn native_math_ranges(input: &str, containers: &[Range<usize>]) -> NativeMathRanges {
    let mut literal_dollars = Vec::new();
    let mut literal_math = Vec::new();
    let mut unclosed_candidates = Vec::new();
    for (event, range) in Parser::new_ext(input, markdown_options()).into_offset_iter() {
        match event {
            Event::Text(_) => {
                for offset in range.filter(|&offset| {
                    input.as_bytes()[offset] == b'$' && !is_escaped(input.as_bytes(), offset)
                }) {
                    literal_dollars.push(offset);
                    if !looks_like_unmatched_literal_dollar(input, offset) {
                        unclosed_candidates.push(offset);
                    }
                }
            }
            Event::InlineMath(source)
                if looks_like_literal_dollars(input, &source, &range)
                    || range.end.checked_sub(1).is_some_and(|close_offset| {
                        crosses_container_boundary(range.start, close_offset, containers)
                    }) =>
            {
                literal_math.push(range);
            }
            _ => {}
        }
    }
    NativeMathRanges {
        literal_dollars,
        literal_math,
        unclosed_candidates,
    }
}

fn looks_like_unmatched_literal_dollar(input: &str, offset: usize) -> bool {
    let suffix = &input[offset + 1..];
    if suffix.starts_with(|character: char| character.is_ascii_digit()) {
        return true;
    }
    let Some((variable, remainder)) = super::split_shell_variable(suffix) else {
        return false;
    };
    if suffix.starts_with('{') {
        return true;
    }

    let obvious_environment_variable = variable.chars().count() > 1
        && variable.chars().all(|character| {
            character == '_' || character.is_ascii_uppercase() || character.is_ascii_digit()
        });
    if !obvious_environment_variable {
        return false;
    }

    let math_tail = remainder.trim_start();
    !math_tail.starts_with(['+', '-', '=', '^', '_', '*', '/', '<', '>', '(', '['])
        && !math_tail.starts_with('\\')
}

fn collect_paired_delimiters(
    input: &str,
    block: &Range<usize>,
    text_ranges: &[Range<usize>],
    containers: &[Range<usize>],
    open: u8,
    close: u8,
) -> (Vec<DelimiterPair>, Option<usize>) {
    let bytes = input.as_bytes();
    let mut pairs = Vec::new();
    let mut open_offset = None;
    let mut index = block.start;
    while index + 1 < block.end {
        if bytes[index] != b'\\'
            || !is_text_source(index + 1..index + 2, text_ranges)
            || is_escaped(bytes, index)
        {
            index += 1;
            continue;
        }

        match (bytes[index + 1], open_offset) {
            (delimiter, None) if delimiter == open => open_offset = Some(index),
            (delimiter, Some(start)) if delimiter == close => {
                if !crosses_container_boundary(start, index, containers) {
                    pairs.push(DelimiterPair {
                        open_offset: start,
                        close_offset: index,
                    });
                }
                open_offset = None;
            }
            _ => {}
        }
        index += 2;
    }
    let unclosed = open_offset.filter(|&start| {
        block
            .end
            .checked_sub(1)
            .is_some_and(|end| !crosses_container_boundary(start, end, containers))
    });
    (pairs, unclosed)
}

fn normalize_multiline_math_whitespace(
    normalized: &mut [u8],
    content_start: usize,
    content_end: usize,
    text_ranges: &[Range<usize>],
) {
    for offset in content_start..content_end {
        if normalized[offset] == b'>'
            && !is_text_source(offset..offset + 1, text_ranges)
            && is_blockquote_marker(normalized, offset)
        {
            normalized[offset] = b' ';
        }
    }
    for byte in &mut normalized[content_start..content_end] {
        if matches!(*byte, b'\r' | b'\n') {
            *byte = b' ';
        }
    }
}

fn is_blockquote_marker(source: &[u8], offset: usize) -> bool {
    let line_start = source[..offset]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |line_end| line_end + 1);
    source[line_start..offset]
        .iter()
        .all(|byte| matches!(*byte, b' ' | b'\t' | b'>'))
}

fn unclosed_native_math_start(block: &Range<usize>, candidates: &[usize]) -> Option<usize> {
    candidates
        .iter()
        .copied()
        .find(|offset| block.contains(offset))
}

fn is_text_source(source: Range<usize>, text_ranges: &[Range<usize>]) -> bool {
    let index = text_ranges.partition_point(|range| range.end <= source.start);
    text_ranges
        .get(index)
        .is_some_and(|range| range.start <= source.start && source.end <= range.end)
}

fn crosses_container_boundary(
    open_offset: usize,
    close_offset: usize,
    containers: &[Range<usize>],
) -> bool {
    containers.iter().any(|range| {
        let open_inside = range.contains(&open_offset);
        let close_inside = range.contains(&close_offset);
        open_inside != close_inside
            || (!open_inside
                && !close_inside
                && open_offset < range.start
                && range.end <= close_offset)
    })
}

fn is_escaped(bytes: &[u8], offset: usize) -> bool {
    let preceding_backslashes = bytes[..offset]
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count();
    !preceding_backslashes.is_multiple_of(2)
}

#[cfg(test)]
#[path = "normalization_tests.rs"]
mod tests;
