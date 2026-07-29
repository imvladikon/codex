//! Streaming markdown render metadata collected during the writer's single parse pass.
//!
//! Top-level block offsets always refer to the exact source passed to this renderer. The supported
//! TeX delimiter normalization is byte-length preserving, so its offsets also index the original
//! source.

use super::DecodedTextMerge;
use super::Event;
use super::HyperlinkLine;
use super::Parser;
use super::Tag;
use super::Writer;
use super::markdown_options;
use super::math;
use super::never_hide_link_destination;
use std::ops::Range;
use std::path::Path;

/// Rendered lines and the block metadata needed to keep only the final block mutable.
pub(crate) struct StreamingMarkdownRender {
    /// Styled output produced by the same parser pass that collected the metadata below.
    pub(crate) lines: Vec<HyperlinkLine>,
    /// Byte offset of the final top-level block when at least one earlier block exists.
    pub(crate) last_top_level_block_start: Option<usize>,
    /// Whether a supported TeX delimiter is still waiting for its closing delimiter.
    pub(crate) has_unclosed_math_delimiter: bool,
    /// Whether a reference definition can retroactively change another block's rendering.
    pub(crate) has_reference_link_definition: bool,
    /// Whether the first block is raw HTML, which joins a retained prefix without a separator.
    pub(crate) first_top_level_block_is_html: bool,
}

/// Render `input` while tracking the final mutable top-level block.
///
/// Every reported byte offset indexes the exact `input` passed here.
pub(crate) fn render_streaming_markdown_lines_with_width_and_cwd(
    input: &str,
    width: Option<usize>,
    cwd: Option<&Path>,
) -> StreamingMarkdownRender {
    let normalized = math::normalize_tex_delimiters(input);
    let parser = Parser::new_ext(&normalized.source, markdown_options());
    let has_reference_link_definition = parser.reference_definitions().iter().next().is_some();
    let parser = TopLevelBlockTracker {
        iter: DecodedTextMerge::new(parser.into_offset_iter()),
        depth: 0,
        block_count: 0,
        last_start: 0,
        first_is_html: false,
    };
    let mut writer = Writer::new(input, parser, width, cwd, &never_hide_link_destination);
    writer.run();
    let last_top_level_block_start = (writer.iter.block_count > 1
        && !normalized.has_unclosed_delimiter)
        .then_some(writer.iter.last_start);
    StreamingMarkdownRender {
        lines: writer.text,
        last_top_level_block_start,
        has_unclosed_math_delimiter: normalized.has_unclosed_delimiter,
        has_reference_link_definition,
        first_top_level_block_is_html: writer.iter.first_is_html,
    }
}

/// Records top-level block boundaries without adding a second parser traversal.
struct TopLevelBlockTracker<I> {
    iter: I,
    depth: usize,
    block_count: usize,
    last_start: usize,
    first_is_html: bool,
}

impl<'a, I> Iterator for TopLevelBlockTracker<I>
where
    I: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    type Item = (Event<'a>, Range<usize>);

    fn next(&mut self) -> Option<Self::Item> {
        let (event, range) = self.iter.next()?;
        if self.depth == 0 && matches!(&event, Event::Start(_) | Event::Rule | Event::Html(_)) {
            self.block_count += 1;
            self.last_start = range.start;
            if self.block_count == 1 {
                self.first_is_html =
                    matches!(&event, Event::Start(Tag::HtmlBlock) | Event::Html(_));
            }
        }
        match event {
            Event::Start(_) => self.depth += 1,
            Event::End(_) => self.depth = self.depth.saturating_sub(1),
            _ => {}
        }
        Some((event, range))
    }
}
