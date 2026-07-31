use super::*;
use pretty_assertions::assert_eq;

#[test]
fn normalizes_native_math_across_blockquote_continuations() {
    let input = "> Formula $x +\n> y\n> z$.\n";
    let markdown = markdown_ranges(input);
    let native = native_math_ranges(input, &markdown.containers);

    assert_eq!(markdown.math_regions, vec![2..25]);
    assert_eq!(native.unclosed_candidates, Vec::<usize>::new());
    assert_eq!(normalize_tex_delimiters(input).source, input);

    let partial = "> Formula $x +\n> y\n";
    assert_eq!(
        normalize_tex_delimiters(partial).unclosed_math_start,
        Some(10),
    );
}

#[test]
fn normalizes_display_math_across_parser_block_boundaries() {
    let input = "\\[\nx\n=\n\n\\left[\ny\n\\right]\n-\n\nz\n\\]";
    let normalized = normalize_tex_delimiters(input);

    assert_eq!(normalized.source, "$$ x =  \\left[ y \\right] -  z $$",);
    assert_eq!(normalized.unclosed_math_start, None);
}

#[test]
fn reports_unclosed_display_math_across_blank_lines() {
    let input = "Stable paragraph.\n\n\\[\nx\n=\n\n";

    assert_eq!(
        normalize_tex_delimiters(input).unclosed_math_start,
        Some("Stable paragraph.\n\n".len()),
    );
}

#[test]
fn display_math_does_not_cross_blockquotes() {
    let input = "\\[\n> protected\n\\]\n";

    assert_eq!(normalize_tex_delimiters(input).source, input);
}

#[test]
fn reports_unclosed_numeric_and_braced_math_expressions() {
    for input in [
        "Formula $1 +\n",
        "Formula $3.14 r^2 +\n",
        "Formula ${x} +\n",
    ] {
        assert_eq!(
            normalize_tex_delimiters(input).unclosed_math_start,
            Some("Formula ".len()),
            "{input:?}",
        );
    }
}

#[test]
fn leaves_unmatched_currency_and_shell_variables_unheld() {
    for input in ["Price $5\n", "Use $HOME\n", "Use ${HOME}\n"] {
        assert_eq!(
            normalize_tex_delimiters(input).unclosed_math_start,
            None,
            "{input:?}",
        );
    }
}
