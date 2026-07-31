pub(super) fn looks_like_unmatched_literal_dollar(input: &str, offset: usize) -> bool {
    let suffix = &input[offset + 1..];
    if suffix.starts_with(|character: char| character.is_ascii_digit()) {
        let numeric_end = suffix
            .char_indices()
            .take_while(|(_, character)| {
                character.is_ascii_digit() || matches!(character, '.' | ',')
            })
            .map(|(offset, character)| offset + character.len_utf8())
            .last()
            .unwrap_or_default();
        return !has_math_expression_tail(&suffix[numeric_end..]);
    }
    let Some((variable, remainder)) = super::super::split_shell_variable(suffix) else {
        return false;
    };
    let environment_variable = variable.chars().count() > 1
        && variable.chars().all(|character| {
            character == '_' || character.is_ascii_uppercase() || character.is_ascii_digit()
        });
    environment_variable && !has_math_expression_tail(remainder)
}

fn has_math_expression_tail(remainder: &str) -> bool {
    let tail = remainder
        .split(['\r', '\n', '$'])
        .next()
        .unwrap_or_default()
        .trim();
    tail.starts_with(['+', '-', '=', '^', '_', '*', '/', '<', '>', '(', '[', '\\'])
        || tail.ends_with(['+', '-', '=', '^', '_', '*', '/', '<', '>'])
        || tail.contains(['^', '_', '\\'])
}
