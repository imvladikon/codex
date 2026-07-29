use super::INLINE_DELIMITER_SENTINEL;
use super::LITERAL_DOLLAR_SENTINEL;
use pulldown_cmark::CodeBlockKind;
use pulldown_cmark::CowStr;
use pulldown_cmark::Event;
use pulldown_cmark::Parser;
use pulldown_cmark::Tag;
use pulldown_cmark::TagEnd;
use std::borrow::Cow;
use std::ops::Range;

pub(in crate::markdown_render) struct NormalizedMath<'a> {
    pub(in crate::markdown_render) source: Cow<'a, str>,
    pub(in crate::markdown_render) has_unclosed_delimiter: bool,
}

struct DelimiterPair {
    open_offset: usize,
    close_offset: usize,
}

pub(in crate::markdown_render) fn restore_literal_dollars(text: CowStr<'_>) -> CowStr<'_> {
    if text.as_bytes().contains(&LITERAL_DOLLAR_SENTINEL) {
        text.replace(LITERAL_DOLLAR_SENTINEL as char, "$").into()
    } else {
        text
    }
}

pub(in crate::markdown_render) fn normalize_tex_delimiters(input: &str) -> NormalizedMath<'_> {
    let protected_ranges = code_ranges(input);
    let (display_pairs, unclosed_display) =
        collect_paired_delimiters(input, &protected_ranges, b'[', b']');
    let (inline_pairs, unclosed_inline) =
        collect_paired_delimiters(input, &protected_ranges, b'(', b')');
    let has_unclosed_delimiter = unclosed_display || unclosed_inline;
    let mut normalized = input.as_bytes().to_vec();
    let neutralized_dollars = neutralize_literal_dollars(input, &mut normalized, &protected_ranges);
    if display_pairs.is_empty() && inline_pairs.is_empty() && !neutralized_dollars {
        return NormalizedMath {
            source: Cow::Borrowed(input),
            has_unclosed_delimiter,
        };
    }

    for pair in display_pairs {
        normalized[pair.open_offset..pair.open_offset + 2].copy_from_slice(b"$$");
        normalized[pair.close_offset..pair.close_offset + 2].copy_from_slice(b"$$");
        normalize_display_whitespace(input, &mut normalized, &pair);
    }
    for pair in inline_pairs {
        normalized[pair.open_offset..pair.open_offset + 2]
            .copy_from_slice(&[b'$', INLINE_DELIMITER_SENTINEL]);
        normalized[pair.close_offset..pair.close_offset + 2]
            .copy_from_slice(&[INLINE_DELIMITER_SENTINEL, b'$']);
    }
    let source = String::from_utf8(normalized)
        .map(Cow::Owned)
        .unwrap_or_else(|_| Cow::Borrowed(input));
    NormalizedMath {
        source,
        has_unclosed_delimiter,
    }
}

fn neutralize_literal_dollars(
    input: &str,
    normalized: &mut [u8],
    protected_ranges: &[Range<usize>],
) -> bool {
    let bytes = input.as_bytes();
    let mut changed = false;
    for index in 0..bytes.len() {
        if bytes[index] != b'$'
            || bytes.get(index.wrapping_sub(1)) == Some(&b'$')
            || bytes.get(index + 1) == Some(&b'$')
            || is_protected(index, protected_ranges)
            || is_escaped(bytes, index)
        {
            continue;
        }
        let Some(next) = bytes.get(index + 1).copied() else {
            continue;
        };
        let looks_literal = next.is_ascii_digit() || next == b'_' || next.is_ascii_uppercase();
        if looks_literal && !has_inline_math_closer(bytes, index, protected_ranges) {
            normalized[index] = LITERAL_DOLLAR_SENTINEL;
            changed = true;
        }
    }
    changed
}

fn has_inline_math_closer(
    bytes: &[u8],
    open_offset: usize,
    protected_ranges: &[Range<usize>],
) -> bool {
    let mut index = open_offset + 1;
    while index < bytes.len() && !matches!(bytes[index], b'\r' | b'\n') {
        if bytes[index] == b'$'
            && bytes.get(index.wrapping_sub(1)) != Some(&b'$')
            && bytes.get(index + 1) != Some(&b'$')
            && !bytes[index - 1].is_ascii_whitespace()
            && !is_protected(index, protected_ranges)
            && !is_escaped(bytes, index)
        {
            return true;
        }
        index += 1;
    }
    false
}

fn code_ranges(input: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut code_block_start = None;
    for (event, range) in Parser::new(input).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_) | CodeBlockKind::Indented)) => {
                code_block_start = Some(range.start);
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(start) = code_block_start.take() {
                    ranges.push(start..range.end);
                }
            }
            Event::Code(_) => ranges.push(range),
            _ => {}
        }
    }
    ranges
}

fn collect_paired_delimiters(
    input: &str,
    protected_ranges: &[Range<usize>],
    open: u8,
    close: u8,
) -> (Vec<DelimiterPair>, bool) {
    let bytes = input.as_bytes();
    let mut pairs = Vec::new();
    let mut open_offset = None;
    let mut index = 0;
    while index + 1 < bytes.len() {
        if bytes[index] != b'\\'
            || is_protected(index, protected_ranges)
            || !is_unescaped_backslash(bytes, index)
        {
            index += 1;
            continue;
        }

        match (bytes[index + 1], open_offset) {
            (delimiter, None) if delimiter == open => open_offset = Some(index),
            (delimiter, Some(start)) if delimiter == close => {
                pairs.push(DelimiterPair {
                    open_offset: start,
                    close_offset: index,
                });
                open_offset = None;
            }
            _ => {}
        }
        index += 2;
    }
    (pairs, open_offset.is_some())
}

fn normalize_display_whitespace(input: &str, normalized: &mut [u8], pair: &DelimiterPair) {
    let bytes = input.as_bytes();
    let source_start = pair.open_offset + 2;
    let source_end = pair.close_offset;
    for byte in &mut normalized[source_start..source_end] {
        if matches!(*byte, b'\r' | b'\n') {
            *byte = b' ';
        }
    }
    if bytes.get(source_start).is_some_and(u8::is_ascii_whitespace) {
        normalized[source_start] = INLINE_DELIMITER_SENTINEL;
    }
    if source_end > source_start
        && bytes
            .get(source_end - 1)
            .is_some_and(u8::is_ascii_whitespace)
    {
        normalized[source_end - 1] = INLINE_DELIMITER_SENTINEL;
    }
}

fn is_protected(offset: usize, ranges: &[Range<usize>]) -> bool {
    ranges.iter().any(|range| range.contains(&offset))
}

fn is_unescaped_backslash(bytes: &[u8], offset: usize) -> bool {
    !is_escaped(bytes, offset)
}

fn is_escaped(bytes: &[u8], offset: usize) -> bool {
    let preceding_backslashes = bytes[..offset]
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count();
    !preceding_backslashes.is_multiple_of(2)
}
