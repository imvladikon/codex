use std::borrow::Cow;
use std::panic::catch_unwind;

pub(super) fn normalize_strict_latex(source: &str) -> Option<Cow<'_, str>> {
    validate_structure(source)?;
    if has_ambiguous_barewords(source) {
        return None;
    }
    let source = normalize_macro_arguments_and_scripts(source)?;
    let source = normalize_latex_aliases(source)?;
    normalize_literal_slashes(source)
}

fn validate_structure(source: &str) -> Option<()> {
    validate_balanced_braces(source)?;

    let mut environments = Vec::new();
    let mut delimiter_depth = 0usize;
    let mut offset = 0;
    while offset < source.len() {
        if source.as_bytes()[offset] == b'\\' {
            if source.as_bytes().get(offset + 1) == Some(&b'\\') {
                if environments.is_empty() {
                    return None;
                }
                offset += 2;
                continue;
            }

            let Some((name, command_end)) = alphabetic_command_at(source, offset) else {
                offset = non_alphabetic_command_end(source, offset)?;
                continue;
            };
            if matches!(name, "begin" | "end") {
                let argument = next_argument(source, command_end)?;
                if !argument.braced {
                    return None;
                }
                let environment = &source[argument.start + 1..argument.end - 1];
                if name == "begin" {
                    environments.push(environment);
                    if environments.len() > super::MAX_MATH_NESTING {
                        return None;
                    }
                } else if environments.pop() != Some(environment) {
                    return None;
                }
                offset = argument.end;
                continue;
            }
            if matches!(name, "left" | "right") {
                let argument = next_argument(source, command_end)?;
                if argument.braced {
                    return None;
                }
                if name == "left" {
                    delimiter_depth = delimiter_depth.checked_add(1)?;
                    if delimiter_depth > super::MAX_MATH_NESTING {
                        return None;
                    }
                } else {
                    delimiter_depth = delimiter_depth.checked_sub(1)?;
                }
                offset = argument.end;
                continue;
            }

            if name == "sqrt"
                && source[command_end..]
                    .trim_start_matches(char::is_whitespace)
                    .starts_with('[')
            {
                return None;
            }

            let argument_count = required_argument_count(name);
            let mut argument_end = command_end;
            for _ in 0..argument_count {
                let argument = next_argument(source, argument_end)?;
                if !argument.braced
                    && source[argument.start..argument.end].starts_with(['(', ')', '^', '_'])
                {
                    return None;
                }
                argument_end = argument.end;
            }
            if matches!(name, "text" | "operatorname")
                && argument_count > 0
                && !next_argument(source, command_end)?.braced
            {
                return None;
            }
            if name == "text" {
                offset = argument_end;
                continue;
            }
            offset = command_end;
            continue;
        }

        if source.as_bytes()[offset] == b'&' && environments.is_empty() {
            return None;
        }
        offset += source[offset..].chars().next()?.len_utf8();
    }

    (environments.is_empty() && delimiter_depth == 0).then_some(())
}

fn normalize_literal_slashes(source: Cow<'_, str>) -> Option<Cow<'_, str>> {
    if !source.contains('/') {
        return Some(source);
    }

    let mut slash_offsets = Vec::new();
    let mut offset = 0;
    while offset < source.len() {
        if source.as_bytes()[offset] == b'\\'
            && let Some((command, command_end)) = alphabetic_command_at(&source, offset)
        {
            if command == "text"
                && let Some(argument) = next_argument(&source, command_end)
            {
                offset = argument.end;
            } else {
                offset = command_end;
            }
            continue;
        }
        if source.as_bytes()[offset] == b'/' && !is_escaped(source.as_bytes(), offset) {
            slash_offsets.push(offset);
        }
        offset += source[offset..].chars().next()?.len_utf8();
    }
    if slash_offsets.is_empty() {
        return Some(source);
    }

    let mut normalized = String::with_capacity(source.len() + slash_offsets.len() * 7);
    let mut copied_until = 0;
    for offset in slash_offsets {
        normalized.push_str(&source[copied_until..offset]);
        normalized.push_str(r"\text{/}");
        copied_until = offset + 1;
    }
    normalized.push_str(&source[copied_until..]);
    Some(Cow::Owned(normalized))
}

fn normalize_latex_aliases(source: Cow<'_, str>) -> Option<Cow<'_, str>> {
    let mut replacements = Vec::new();
    let mut offset = 0;
    while offset < source.len() {
        if source.as_bytes()[offset] != b'\\' {
            offset += source[offset..].chars().next()?.len_utf8();
            continue;
        }
        if source[offset..].starts_with(r"\|") {
            replacements.push((offset..offset + 2, "‖".to_string()));
            offset += 2;
            continue;
        }
        let Some((command, command_end)) = alphabetic_command_at(&source, offset) else {
            offset += 1;
            continue;
        };
        if matches!(command, "text" | "operatorname")
            && let Some(argument) = next_argument(&source, command_end)
        {
            offset = argument.end;
            continue;
        }
        if matches!(command, "xrightarrow" | "xleftarrow") {
            let argument = next_argument(&source, command_end)?;
            let arrow = if command == "xrightarrow" {
                r"\to"
            } else {
                r"\leftarrow"
            };
            let argument_source = if argument.braced {
                &source[argument.start + 1..argument.end - 1]
            } else {
                &source[argument.start..argument.end]
            };
            replacements.push((
                offset..argument.end,
                format!(r"\overset{{{argument_source}}}{{{arrow}}}"),
            ));
            offset = argument.end;
            continue;
        }
        let replacement = match command {
            "tfrac" | "dfrac" => r"\frac",
            "widehat" => r"\hat",
            "Box" => "□",
            "dagger" => "†",
            "langle" => "⟨",
            "rangle" => "⟩",
            "vert" | "lvert" | "rvert" => "|",
            "mid" => " | ",
            "Vert" | "lVert" | "rVert" => "‖",
            "big" | "Big" | "bigg" | "Bigg" | "bigl" | "bigr" | "Bigl" | "Bigr" | "biggl"
            | "biggr" | "Biggl" | "Biggr" => "",
            "Longleftrightarrow" => r"\Leftrightarrow",
            "longrightarrow" => r"\to",
            "longleftarrow" => r"\leftarrow",
            _ => {
                offset = command_end;
                continue;
            }
        };
        replacements.push((offset..command_end, replacement.to_string()));
        offset = command_end;
    }
    if replacements.is_empty() {
        return Some(source);
    }

    let replacement_bytes = replacements
        .iter()
        .map(|(_, replacement)| replacement.len())
        .sum::<usize>();
    let mut normalized = String::with_capacity(source.len() + replacement_bytes);
    let mut copied_until = 0;
    for (range, replacement) in replacements {
        normalized.push_str(&source[copied_until..range.start]);
        normalized.push_str(&replacement);
        copied_until = range.end;
    }
    normalized.push_str(&source[copied_until..]);
    Some(Cow::Owned(normalized))
}

fn validate_balanced_braces(source: &str) -> Option<()> {
    let mut depth = 0usize;
    for (offset, character) in source.char_indices() {
        if is_escaped(source.as_bytes(), offset) {
            continue;
        }
        match character {
            '{' => depth += 1,
            '}' => depth = depth.checked_sub(1)?,
            _ => {}
        }
    }
    (depth == 0).then_some(())
}

fn required_argument_count(command: &str) -> usize {
    match command {
        "frac" | "tfrac" | "dfrac" | "binom" | "overset" | "underset" | "stackrel" => 2,
        "sqrt" | "text" | "mathbf" | "mathbb" | "mathcal" | "mathrm" | "mathfrak" | "mathsf"
        | "mathtt" | "hat" | "widehat" | "bar" | "overline" | "dot" | "ddot" | "tilde" | "vec"
        | "overbrace" | "underbrace" | "operatorname" | "boxed" | "xrightarrow" | "xleftarrow" => 1,
        _ => 0,
    }
}

#[derive(Clone, Copy)]
struct Argument {
    start: usize,
    end: usize,
    braced: bool,
}

fn next_argument(source: &str, offset: usize) -> Option<Argument> {
    let start = skip_whitespace(source, offset);
    let first = source[start..].chars().next()?;
    if first == '{' {
        return matching_brace_end(source, start).map(|end| Argument {
            start,
            end,
            braced: true,
        });
    }
    if first == '\\' {
        let end = if let Some((_, end)) = alphabetic_command_at(source, start) {
            end
        } else {
            start
                + first.len_utf8()
                + source[start + first.len_utf8()..]
                    .chars()
                    .next()?
                    .len_utf8()
        };
        return Some(Argument {
            start,
            end,
            braced: false,
        });
    }
    Some(Argument {
        start,
        end: start + first.len_utf8(),
        braced: false,
    })
}

fn matching_brace_end(source: &str, start: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (relative_offset, character) in source[start..].char_indices() {
        let offset = start + relative_offset;
        if is_escaped(source.as_bytes(), offset) {
            continue;
        }
        match character {
            '{' => depth += 1,
            '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(offset + character.len_utf8());
                }
            }
            _ => {}
        }
    }
    None
}

fn normalize_macro_arguments_and_scripts(source: &str) -> Option<Cow<'_, str>> {
    let mut insertions = Vec::new();
    let mut offset = 0;
    while offset < source.len() {
        if matches!(source.as_bytes()[offset], b'^' | b'_') {
            let argument = next_argument(source, offset + 1)?;
            record_unbraced_argument(source, argument, &mut insertions)?;
            offset += 1;
            continue;
        }
        if source.as_bytes()[offset] != b'\\' {
            offset += source[offset..].chars().next()?.len_utf8();
            continue;
        }
        let Some((command, command_end)) = alphabetic_command_at(source, offset) else {
            offset += 1;
            continue;
        };
        let argument_count = required_argument_count(command);
        if argument_count == 0 {
            offset = command_end;
            continue;
        }

        let mut argument_end = command_end;
        for _ in 0..argument_count {
            let argument = next_argument(source, argument_end)?;
            record_unbraced_argument(source, argument, &mut insertions)?;
            argument_end = argument.end;
        }
        offset = if command == "text" {
            argument_end
        } else {
            command_end
        };
    }

    if insertions.is_empty() {
        return Some(Cow::Borrowed(source));
    }
    insertions.sort_by_key(|(offset, insertion)| (*offset, *insertion));
    let mut normalized = String::with_capacity(source.len() + insertions.len());
    let mut copied_until = 0;
    for (offset, insertion) in insertions {
        normalized.push_str(&source[copied_until..offset]);
        normalized.push(match insertion {
            Insertion::Close => '}',
            Insertion::Open => '{',
        });
        copied_until = offset;
    }
    normalized.push_str(&source[copied_until..]);
    Some(Cow::Owned(normalized))
}

fn record_unbraced_argument(
    source: &str,
    argument: Argument,
    insertions: &mut Vec<(usize, Insertion)>,
) -> Option<()> {
    if argument.braced {
        return Some(());
    }
    if source[argument.start..argument.end].starts_with('\\')
        && alphabetic_command_at(source, argument.start)
            .is_some_and(|(name, _)| required_argument_count(name) > 0)
    {
        return None;
    }
    insertions.push((argument.start, Insertion::Open));
    insertions.push((argument.end, Insertion::Close));
    Some(())
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum Insertion {
    Close,
    Open,
}

fn has_ambiguous_barewords(source: &str) -> bool {
    let mut offset = 0;
    while offset < source.len() {
        if source.as_bytes()[offset] == b'\\' {
            let Some((command, command_end)) = alphabetic_command_at(source, offset) else {
                offset += 1;
                continue;
            };
            if command == "text"
                && let Some(argument) = next_argument(source, command_end)
            {
                offset = argument.end;
            } else {
                offset = command_end;
            }
            continue;
        }

        let Some(character) = source[offset..].chars().next() else {
            break;
        };
        if !character.is_ascii_alphabetic() {
            offset += character.len_utf8();
            continue;
        }
        let word_end = source[offset..]
            .char_indices()
            .take_while(|(_, character)| character.is_ascii_alphabetic())
            .map(|(relative_offset, character)| offset + relative_offset + character.len_utf8())
            .last()
            .unwrap_or(offset + character.len_utf8());
        let word = &source[offset..word_end];
        let introduces_command = catch_unwind(|| term_maths::to_latex(word))
            .map(|round_trip| contains_alphabetic_command(&round_trip))
            .unwrap_or(true);
        if introduces_command {
            return true;
        }
        offset = word_end;
    }
    false
}

fn contains_alphabetic_command(source: &str) -> bool {
    source
        .as_bytes()
        .windows(2)
        .any(|pair| pair[0] == b'\\' && pair[1].is_ascii_alphabetic())
}

fn alphabetic_command_at(source: &str, offset: usize) -> Option<(&str, usize)> {
    if source.as_bytes().get(offset) != Some(&b'\\') {
        return None;
    }
    let start = offset + 1;
    let end = source[start..]
        .char_indices()
        .take_while(|(_, character)| character.is_ascii_alphabetic())
        .map(|(relative_offset, character)| start + relative_offset + character.len_utf8())
        .last()?;
    Some((&source[start..end], end))
}

fn non_alphabetic_command_end(source: &str, offset: usize) -> Option<usize> {
    let after_backslash = offset + 1;
    let character = source[after_backslash..].chars().next()?;
    Some(after_backslash + character.len_utf8())
}

fn skip_whitespace(source: &str, mut offset: usize) -> usize {
    while let Some(character) = source[offset..].chars().next() {
        if !character.is_whitespace() {
            break;
        }
        offset += character.len_utf8();
    }
    offset
}

fn is_escaped(bytes: &[u8], offset: usize) -> bool {
    !bytes[..offset]
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count()
        .is_multiple_of(2)
}

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;
