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

    let (text_ranges, containers) = markdown_ranges(input);
    let mut native_math = native_math_ranges(input);
    native_math
        .literal_dollars
        .retain(|&offset| is_text_source(offset..offset + 1, &text_ranges));
    let mut pairs = Vec::new();
    let mut unclosed_math_start = None;
    for region in math_regions(input) {
        let (display_pairs, unclosed_display) =
            collect_paired_delimiters(input, &region, &text_ranges, &containers, b'[', b']');
        let (inline_pairs, unclosed_inline) =
            collect_paired_delimiters(input, &region, &text_ranges, &containers, b'(', b')');
        pairs.extend(display_pairs.into_iter().map(|pair| (pair, b'[', b']')));
        pairs.extend(inline_pairs.into_iter().map(|pair| (pair, b'(', b')')));

        if region.end == input.len() {
            unclosed_math_start = [
                unclosed_display,
                unclosed_inline,
                unclosed_native_math_start(&region, &native_math.literal_dollars),
            ]
            .into_iter()
            .flatten()
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
        if is_text_source(close_offset..range.end, &text_ranges) {
            normalized[close_offset] = LITERAL_DOLLAR_SENTINEL;
        }
    }
    for (pair, open, close) in pairs {
        match (open, close) {
            (b'[', b']') => {
                normalize_display_whitespace(&mut normalized, &pair);
                normalized[pair.open_offset..pair.open_offset + 2].copy_from_slice(b"$$");
                normalized[pair.close_offset..pair.close_offset + 2].copy_from_slice(b"$$");
            }
            (b'(', b')') => {
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

fn markdown_ranges(input: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let mut text_ranges = Vec::new();
    let mut containers = Vec::new();
    let mut protected_depth = 0usize;
    let mut options = markdown_options();
    options.remove(Options::ENABLE_MATH);
    for (event, range) in Parser::new_ext(input, options).into_offset_iter() {
        match event {
            Event::Start(
                Tag::CodeBlock(_) | Tag::HtmlBlock | Tag::Image { .. } | Tag::MetadataBlock(_),
            ) => protected_depth += 1,
            Event::End(
                TagEnd::CodeBlock | TagEnd::HtmlBlock | TagEnd::Image | TagEnd::MetadataBlock(_),
            ) => protected_depth = protected_depth.saturating_sub(1),
            Event::Start(Tag::Link { .. } | Tag::TableCell) => containers.push(range),
            Event::Text(_) if protected_depth == 0 => text_ranges.push(range),
            _ => {}
        }
    }
    (text_ranges, containers)
}

fn native_math_ranges(input: &str) -> NativeMathRanges {
    let mut literal_dollars = Vec::new();
    let mut literal_math = Vec::new();
    for (event, range) in Parser::new_ext(input, markdown_options()).into_offset_iter() {
        match event {
            Event::Text(_) => {
                literal_dollars.extend(range.filter(|&offset| {
                    input.as_bytes()[offset] == b'$' && !is_escaped(input.as_bytes(), offset)
                }));
            }
            Event::InlineMath(source) if looks_like_literal_dollars(input, &source, &range) => {
                literal_math.push(range);
            }
            _ => {}
        }
    }
    NativeMathRanges {
        literal_dollars,
        literal_math,
    }
}

fn math_regions(input: &str) -> Vec<Range<usize>> {
    let mut regions = Vec::new();
    let mut region_start = None;
    let mut line_start = 0;
    for line in input.split_inclusive('\n') {
        let line_without_ending = line.strip_suffix('\n').unwrap_or(line);
        let line_without_ending = line_without_ending
            .strip_suffix('\r')
            .unwrap_or(line_without_ending);
        if line_without_ending.trim().is_empty() {
            if let Some(start) = region_start.take() {
                regions.push(start..line_start);
            }
        } else if is_markdown_block_start(line_without_ending)
            && let Some(start) = region_start.replace(line_start)
            && start < line_start
        {
            regions.push(start..line_start);
        } else {
            region_start.get_or_insert(line_start);
        }
        line_start += line.len();
    }
    if let Some(start) = region_start {
        regions.push(start..input.len());
    }
    regions
}

fn is_markdown_block_start(line: &str) -> bool {
    let indentation = line
        .len()
        .saturating_sub(line.trim_start_matches(' ').len());
    if indentation >= 4 || line[indentation..].starts_with('\t') {
        return true;
    }
    let line = &line[indentation..];
    let heading = line.starts_with('#')
        && line
            .trim_start_matches('#')
            .strip_prefix([' ', '\t'])
            .is_some()
        && line
            .len()
            .saturating_sub(line.trim_start_matches('#').len())
            <= 6;
    let unordered_list = ["- ", "* ", "+ "]
        .into_iter()
        .any(|marker| line.starts_with(marker));
    let ordered_list = line.find(['.', ')']).is_some_and(|offset| {
        offset > 0
            && line[..offset].bytes().all(|byte| byte.is_ascii_digit())
            && line[offset + 1..].starts_with([' ', '\t'])
    });
    let thematic_break = ['*', '-', '_'].into_iter().any(|marker| {
        line.chars()
            .all(|character| character == marker || matches!(character, ' ' | '\t'))
            && line
                .chars()
                .filter(|character| *character == marker)
                .count()
                >= 3
    });
    heading
        || unordered_list
        || ordered_list
        || thematic_break
        || line.starts_with('>')
        || line.starts_with("```")
        || line.starts_with("~~~")
        || line.starts_with('|')
        || line.starts_with('<')
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
    (pairs, open_offset)
}

fn normalize_display_whitespace(normalized: &mut [u8], pair: &DelimiterPair) {
    for byte in &mut normalized[pair.open_offset + 2..pair.close_offset] {
        if matches!(*byte, b'\r' | b'\n') {
            *byte = b' ';
        }
    }
}

fn unclosed_native_math_start(block: &Range<usize>, literal_dollars: &[usize]) -> Option<usize> {
    literal_dollars
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
