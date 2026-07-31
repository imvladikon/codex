use super::super::MAX_MATH_COLUMNS;
use super::super::MAX_MATH_ROWS;
use crate::width::display_width;

#[derive(Clone)]
pub(super) struct Block {
    pub(super) rows: Vec<String>,
    pub(super) baseline: usize,
}

impl Block {
    pub(super) fn new(rows: Vec<String>, baseline: usize) -> Option<Self> {
        if rows.is_empty()
            || rows.len() > MAX_MATH_ROWS
            || baseline >= rows.len()
            || rows.iter().any(|row| display_width(row) > MAX_MATH_COLUMNS)
        {
            return None;
        }
        Some(Self { rows, baseline })
    }

    pub(super) fn empty() -> Self {
        Self {
            rows: vec![String::new()],
            baseline: 0,
        }
    }

    pub(super) fn text(text: impl Into<String>) -> Option<Self> {
        Self::new(vec![text.into()], 0)
    }

    pub(super) fn width(&self) -> usize {
        self.rows
            .iter()
            .map(|row| display_width(row))
            .max()
            .unwrap_or(0)
    }

    pub(super) fn beside(&self, other: &Self) -> Option<Self> {
        let left_width = self.width();
        let right_width = other.width();
        if left_width
            .checked_add(right_width)
            .is_none_or(|width| width > MAX_MATH_COLUMNS)
        {
            return None;
        }
        let baseline = self.baseline.max(other.baseline);
        let left_top = baseline - self.baseline;
        let right_top = baseline - other.baseline;
        let height = (left_top + self.rows.len()).max(right_top + other.rows.len());
        let rows = (0..height)
            .map(|row_index| {
                format!(
                    "{}{}",
                    pad_right(row_in_block(self, row_index, left_top), left_width),
                    pad_right(row_in_block(other, row_index, right_top), right_width)
                )
            })
            .collect();
        Self::new(rows, baseline)
    }
}

fn row_in_block(block: &Block, row_index: usize, top: usize) -> &str {
    row_index
        .checked_sub(top)
        .and_then(|index| block.rows.get(index))
        .map(String::as_str)
        .unwrap_or_default()
}

pub(super) fn pad_right(row: &str, width: usize) -> String {
    format!(
        "{row}{}",
        " ".repeat(width.saturating_sub(display_width(row)))
    )
}

pub(super) fn center(row: &str, width: usize) -> String {
    let padding = width.saturating_sub(display_width(row));
    format!(
        "{}{row}{}",
        " ".repeat(padding / 2),
        " ".repeat(padding - padding / 2)
    )
}
