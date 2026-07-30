use super::*;

#[test]
fn rejects_each_oversized_resource_class() {
    assert!(render(&"x".repeat(MAX_MATH_SOURCE_BYTES + 1)).is_none());

    let excessive_nesting = format!(
        "{}x{}",
        "{".repeat(MAX_MATH_NESTING + 1),
        "}".repeat(MAX_MATH_NESTING + 1),
    );
    assert!(render(&excessive_nesting).is_none());

    let excessive_rows = std::iter::repeat_n("x", MAX_MATH_ROWS + 1)
        .collect::<Vec<_>>()
        .join(r" \\ ");
    assert!(render(&format!(r"\begin{{matrix}}{excessive_rows}\end{{matrix}}")).is_none());

    let excessive_columns = std::iter::repeat_n("x", MAX_MATH_COLUMNS)
        .collect::<Vec<_>>()
        .join("+");
    assert!(render(&excessive_columns).is_none());
}
