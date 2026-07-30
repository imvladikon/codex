use super::StreamingRender;
use super::render_source;
use crate::history_cell::HistoryRenderMode;
use crate::inline_visualization::InlineVisualizationContext;
use crate::markdown::render_streaming_markdown_agent_with_links_and_cwd;
use crate::terminal_hyperlinks::HyperlinkLine;
use crate::terminal_hyperlinks::visible_lines;
use codex_protocol::ThreadId;
use insta::assert_debug_snapshot;
use pretty_assertions::assert_eq;
use std::path::Path;
use std::path::PathBuf;

fn test_cwd() -> PathBuf {
    std::env::temp_dir()
}

fn append(
    render: &mut StreamingRender,
    source: &mut String,
    chunk: &str,
    width: Option<usize>,
    cwd: &Path,
    render_mode: HistoryRenderMode,
) {
    source.push_str(chunk);
    render.append(
        source,
        chunk,
        width,
        cwd,
        render_mode,
        /*inline_visualization_context*/ None,
    );
}

fn append_rich_and_assert_matches_full(
    render: &mut StreamingRender,
    source: &mut String,
    chunk: &str,
    width: Option<usize>,
    cwd: &Path,
) {
    append(render, source, chunk, width, cwd, HistoryRenderMode::Rich);
    assert_eq!(
        render.lines,
        render_source(
            source,
            width,
            cwd,
            HistoryRenderMode::Rich,
            /*inline_visualization_context*/ None,
        ),
        "incremental render diverged after chunk {chunk:?}",
    );
}

fn lines_to_plain_strings(lines: &[HyperlinkLine]) -> Vec<String> {
    visible_lines(lines.to_vec())
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect()
}

fn assert_rich_stream_matches_full_render(
    chunks: &[&str],
    width: Option<usize>,
) -> (String, StreamingRender) {
    let cwd = test_cwd();
    let mut source = String::new();
    let mut render = StreamingRender::new();

    for chunk in chunks {
        append_rich_and_assert_matches_full(&mut render, &mut source, chunk, width, &cwd);
    }

    (source, render)
}

#[test]
fn incremental_render_keeps_final_block_mutable_and_matches_full_render() {
    let chunks = [
        "# Heading\n",
        "\n",
        "First paragraph with a [link](https://example.com).\n",
        "continued on the next line.\n\n",
        "1. First item\n",
        "2. Second item\n\n",
        "> Quoted paragraph\n\n",
        "```rust\n",
        "fn main() {}\n",
        "```\n\n",
        "| Key | Value |\n",
        "| --- | --- |\n",
        "| alpha | beta |\n",
    ];
    let (source, render) = assert_rich_stream_matches_full_render(&chunks, Some(48));

    assert!(render.stable_source_len > 0);
    assert!(render.stable_source_len < source.len());
    assert_debug_snapshot!("incremental_render_representative_stream", render.lines);
}

#[test]
fn incremental_render_preserves_literal_dollars() {
    let (_, render) = assert_rich_stream_matches_full_render(
        &["Price $5 and ", "$10; shell ", "$HOME; code ", "`$x^2$`.\n"],
        Some(/*width*/ 80),
    );

    assert_eq!(
        lines_to_plain_strings(&render.lines),
        vec!["Price $5 and $10; shell $HOME; code $x^2$."],
    );
}

#[test]
fn incremental_render_tracks_block_containing_unclosed_native_math() {
    let cwd = test_cwd();
    let width = Some(80);
    let mut source = String::new();
    let mut render = StreamingRender::new();
    let prefix = "Completed paragraph.\n\nFormula ";
    let formula_block_start = "Completed paragraph.\n\n".len();

    append(
        &mut render,
        &mut source,
        &format!("{prefix}$x"),
        width,
        &cwd,
        HistoryRenderMode::Rich,
    );
    assert_eq!(render.unclosed_math_start, Some(formula_block_start));

    append(
        &mut render,
        &mut source,
        "$ is complete.\n",
        width,
        &cwd,
        HistoryRenderMode::Rich,
    );
    assert_eq!(render.unclosed_math_start, None);
    assert_eq!(
        render.lines,
        render_source(
            &source,
            width,
            &cwd,
            HistoryRenderMode::Rich,
            /*inline_visualization_context*/ None,
        ),
    );
}

#[test]
fn completed_paragraph_does_not_hold_unmatched_literal_dollar() {
    let (_, render) = assert_rich_stream_matches_full_render(
        &["Shell variable $HOME.\n\n", "Following paragraph.\n"],
        Some(/*width*/ 80),
    );

    assert_eq!(render.unclosed_math_start, None);
    assert!(render.stable_source_len > 0);
}

#[test]
fn growing_paragraph_does_not_hold_unmatched_shell_variable() {
    let (_, render) = assert_rich_stream_matches_full_render(
        &[
            "Use echo $HOME\n",
            "Then run the next command\n",
            "And continue explaining...",
        ],
        Some(/*width*/ 80),
    );

    assert_eq!(render.unclosed_math_start, None);
}

#[test]
fn blockquoted_math_stays_mutable_until_its_closing_delimiter() {
    let cwd = test_cwd();
    let mut source = String::new();
    let mut render = StreamingRender::new();

    append_rich_and_assert_matches_full(
        &mut render,
        &mut source,
        "> Formula $x +\n",
        Some(/*width*/ 80),
        &cwd,
    );
    assert_eq!(render.unclosed_math_start, Some(0));
    append_rich_and_assert_matches_full(
        &mut render,
        &mut source,
        "> y\n",
        Some(/*width*/ 80),
        &cwd,
    );
    assert_eq!(render.unclosed_math_start, Some(0));
    append_rich_and_assert_matches_full(
        &mut render,
        &mut source,
        "> z$.\n",
        Some(/*width*/ 80),
        &cwd,
    );
    assert_eq!(render.unclosed_math_start, None);
    let lines = lines_to_plain_strings(&render.lines);
    assert!(
        lines.join("\n").contains("x + y z"),
        "unexpected blockquote render: {lines:?}",
    );
}

#[test]
fn recompute_preserves_unclosed_math_byte_offset() {
    let cwd = test_cwd();
    let width = Some(80);
    let prefix = "Préfixe terminé.\n\n";
    let mut source = format!("{prefix}Formula $x");
    let mut render = StreamingRender::new();

    append(
        &mut render,
        &mut source,
        "",
        width,
        &cwd,
        HistoryRenderMode::Rich,
    );
    assert_eq!(render.unclosed_math_start, Some(prefix.len()));

    render.recompute(
        &source,
        Some(32),
        &cwd,
        HistoryRenderMode::Rich,
        /*inline_visualization_context*/ None,
    );
    assert_eq!(render.unclosed_math_start, Some(prefix.len()));

    append_rich_and_assert_matches_full(
        &mut render,
        &mut source,
        "$ is complete.\n",
        Some(32),
        &cwd,
    );
    assert_eq!(render.unclosed_math_start, None);
}

#[test]
fn multiline_tex_display_stays_mutable_across_blank_lines() {
    let cwd = test_cwd();
    let width = Some(120);
    let prefix = "Completed paragraph.\n\n";
    let mut source = String::new();
    let mut render = StreamingRender::new();

    append_rich_and_assert_matches_full(
        &mut render,
        &mut source,
        &format!("{prefix}\\[\n\\int_a^b f(x)\\,dx\n=\n\n"),
        width,
        &cwd,
    );
    assert_eq!(render.unclosed_math_start, Some(prefix.len()));

    append_rich_and_assert_matches_full(
        &mut render,
        &mut source,
        "\\left[F(x)\\right]_a^b\n-\n\nC\n",
        width,
        &cwd,
    );
    assert_eq!(render.unclosed_math_start, Some(prefix.len()));

    append_rich_and_assert_matches_full(&mut render, &mut source, "\\]\n", width, &cwd);
    assert_eq!(render.unclosed_math_start, None);
    let rendered = lines_to_plain_strings(&render.lines).join("\n");
    assert!(
        !rendered.contains(r"\[") && !rendered.contains("# ["),
        "{rendered}"
    );
}

#[test]
fn protected_tex_openers_do_not_hold_streaming_output() {
    for protected in [
        r"`\(`",
        "```\n\\[\n```",
        r"[label](<https://example.test/\(>)",
        r"![\(](image.png)",
        r#"<span data-math="\(">html</span>"#,
        r"\\(",
    ] {
        let (_, render) = assert_rich_stream_matches_full_render(
            &[&format!("{protected}\n\n"), "Stable paragraph.\n"],
            Some(/*width*/ 80),
        );

        assert_eq!(render.unclosed_math_start, None, "{protected:?}");
        assert!(
            render.stable_source_len > 0,
            "stable prose was not committed after {protected:?}",
        );
    }
}

#[test]
fn growing_single_top_level_blocks_render_and_scan_in_one_pass() {
    let streams: &[&[&str]] = &[
        &[
            "A paragraph that keeps growing\n",
            "without a blank line between chunks.\n",
            "It stays one top-level block.\n",
        ],
        &[
            "| Key | Value |\n",
            "| --- | --- |\n",
            "| alpha | beta |\n",
            "| gamma | delta |\n",
        ],
    ];
    let cwd = test_cwd();
    let width = Some(80);

    for chunks in streams {
        let mut source = String::new();
        let mut render = StreamingRender::new();
        for chunk in *chunks {
            append_rich_and_assert_matches_full(&mut render, &mut source, chunk, width, &cwd);
            let pending = render_streaming_markdown_agent_with_links_and_cwd(
                &source,
                width,
                Some(cwd.as_path()),
            );
            assert_eq!(pending.last_top_level_block_start, None);
            assert_eq!(render.lines, pending.lines);
            assert_eq!(render.stable_source_len, 0);
        }
    }
}

#[test]
fn incremental_raw_render_preserves_blank_lines() {
    let cwd = test_cwd();
    let width = Some(80);
    let mut source = String::new();
    let mut render = StreamingRender::new();

    for chunk in ["alpha\n", "\n", "beta\n", "\n"] {
        append(
            &mut render,
            &mut source,
            chunk,
            width,
            &cwd,
            HistoryRenderMode::Raw,
        );
    }

    assert_eq!(
        lines_to_plain_strings(&render.lines),
        vec!["alpha", "", "beta", ""],
    );
}

#[test]
fn inline_visualization_context_without_directives_keeps_stable_prefix() {
    let cwd = test_cwd();
    let context = InlineVisualizationContext::new(&cwd, ThreadId::new())
        .expect("UUIDv7 thread id should provide a timestamp");
    let width = Some(80);
    let mut source = String::new();
    let mut render = StreamingRender::new();

    for chunk in ["First paragraph.\n\n", "Second paragraph.\n\n"] {
        source.push_str(chunk);
        render.append(
            &source,
            chunk,
            width,
            &cwd,
            HistoryRenderMode::Rich,
            Some(&context),
        );
        assert_eq!(
            render.lines,
            render_source(
                &source,
                width,
                &cwd,
                HistoryRenderMode::Rich,
                Some(&context),
            ),
        );
    }

    assert!(render.stable_source_len > 0);
    assert!(!render.has_inline_visualization_directive);
}

#[test]
fn inline_visualizations_use_canonical_full_render() {
    let cwd = test_cwd();
    let context = InlineVisualizationContext::new(&cwd, ThreadId::new())
        .expect("UUIDv7 thread id should provide a timestamp");
    let width = Some(80);
    let mut source = String::new();
    let mut render = StreamingRender::new();

    for chunk in ["Before.\n\n", "::codex-inline-vis{file=\"missing.html\"}\n"] {
        source.push_str(chunk);
        render.append(
            &source,
            chunk,
            width,
            &cwd,
            HistoryRenderMode::Rich,
            Some(&context),
        );
        assert_eq!(
            render.lines,
            render_source(
                &source,
                width,
                &cwd,
                HistoryRenderMode::Rich,
                Some(&context),
            ),
        );
        assert_eq!(render.stable_source_len, 0);
    }

    assert!(render.has_inline_visualization_directive);
}

#[test]
fn inline_visualizations_without_context_use_canonical_full_render() {
    let (_, render) = assert_rich_stream_matches_full_render(
        &["Before.\n\n", "::codex-inline-vis{file=\"missing.html\"}\n"],
        Some(80),
    );

    assert_eq!(render.stable_source_len, 0);
    assert!(render.has_inline_visualization_directive);
    assert_debug_snapshot!(
        "inline_visualizations_without_context_use_canonical_full_render",
        render.lines
    );
}

#[test]
fn inline_visualization_directive_survives_raw_to_rich_render_mode_switch() {
    let cwd = test_cwd();
    let width = Some(80);
    let mut source = String::new();
    let mut render = StreamingRender::new();

    append(
        &mut render,
        &mut source,
        "::codex-inline-vis{file=\"missing.html\"}\n",
        width,
        &cwd,
        HistoryRenderMode::Raw,
    );
    assert!(!render.has_inline_visualization_directive);

    render.recompute(
        &source,
        width,
        &cwd,
        HistoryRenderMode::Rich,
        /*inline_visualization_context*/ None,
    );

    assert!(render.has_inline_visualization_directive);
    assert_eq!(
        render.lines,
        render_source(
            &source,
            width,
            &cwd,
            HistoryRenderMode::Rich,
            /*inline_visualization_context*/ None,
        ),
    );

    render.clear();
    assert!(!render.has_inline_visualization_directive);
}

#[test]
fn reference_link_definition_recomputes_earlier_and_later_blocks() {
    let streams: &[&[&str]] = &[
        &[
            "Earlier [reference][id].\n\n",
            "An unrelated paragraph.\n\n",
            "[id]: https://example.com/reference\n",
            "\n",
            "Later [reference][id].\n",
        ],
        &[
            "Earlier [reference][id].\n\n",
            "Another paragraph.\n\n",
            "```markdown\n",
            "| Key | Value |\n",
            "| --- | --- |\n",
            "| alpha | beta |\n",
            "\n",
            "[id]: https://example.com/reference\n",
            "```\n",
        ],
    ];
    for chunks in streams {
        let (_, render) = assert_rich_stream_matches_full_render(chunks, Some(80));
        assert!(render.has_reference_link_definition);
    }
}

#[test]
fn reference_link_definition_survives_raw_to_rich_render_mode_switch() {
    let cwd = test_cwd();
    let width = Some(80);
    let mut source = String::new();
    let mut render = StreamingRender::new();

    append(
        &mut render,
        &mut source,
        "[id]: https://example.com/reference\n\n",
        width,
        &cwd,
        HistoryRenderMode::Raw,
    );
    render.recompute(
        &source,
        width,
        &cwd,
        HistoryRenderMode::Rich,
        /*inline_visualization_context*/ None,
    );
    for chunk in ["First [reference][id].\n\n", "Later [reference][id].\n"] {
        append_rich_and_assert_matches_full(&mut render, &mut source, chunk, width, &cwd);
    }

    assert!(render.has_reference_link_definition);
}

#[test]
fn incremental_render_does_not_add_blank_line_before_html_block() {
    let streams: &[&[&str]] = &[
        &[
            "Paragraph.\n\n",
            "<div>x</div>\n\n",
            "Following paragraph.\n",
        ],
        &["Paragraph.\n\n<div>x</div>\n\n", "Following paragraph.\n"],
    ];
    for chunks in streams {
        let (_, render) = assert_rich_stream_matches_full_render(chunks, Some(80));
        assert_eq!(
            lines_to_plain_strings(&render.lines),
            vec!["Paragraph.", "<div>x</div>", "", "Following paragraph.",],
        );
    }
}

#[test]
fn incremental_render_preserves_heading_and_normalized_html_seams() {
    let streams: &[&[&str]] = &[
        &["Paragraph.\n\n# Heading\n\n", "Next paragraph.\n"],
        &[
            "Paragraph.\n\n",
            "<div>x</div>\n\n",
            "```markdown\n",
            "| Key | Value |\n",
            "| --- | --- |\n",
            "| alpha | beta |\n",
            "\n",
            "Following paragraph.\n",
            "```\n",
        ],
    ];
    for chunks in streams {
        assert_rich_stream_matches_full_render(chunks, Some(80));
    }
}

#[test]
fn paragraphs_after_unwrapped_table_fence_advance_stable_source() {
    let cwd = test_cwd();
    let width = Some(80);
    let (mut source, mut render) = assert_rich_stream_matches_full_render(
        &[
            "```markdown\n",
            "| Key | Value |\n",
            "| --- | --- |\n",
            "| alpha | beta |\n",
            "```\n\n",
        ],
        width,
    );

    let mut previous_stable_source_len = render.stable_source_len;
    for block in ["<div>Post-fence HTML.</div>\n\n", "First paragraph.\n\n"] {
        append_rich_and_assert_matches_full(&mut render, &mut source, block, width, &cwd);
        assert!(render.stable_source_len > previous_stable_source_len);
        previous_stable_source_len = render.stable_source_len;
    }
}
