use super::*;

#[test]
fn rejects_each_oversized_resource_class() {
    assert!(render(&"x".repeat(MAX_MATH_SOURCE_BYTES + 1)).is_none());

    let excessive_depth = MAX_MATH_ROWS * 4;
    let excessive_nesting = format!(
        "{}x{}",
        "{".repeat(excessive_depth),
        "}".repeat(excessive_depth),
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

    let excessive_composition = r"\boxed{x}".repeat(MAX_MATH_COLUMNS);
    assert!(render(&excessive_composition).is_none());
}

#[test]
fn rejects_excessive_delimiter_and_environment_depth() {
    let delimiter_depth = MAX_MATH_ROWS * 4;
    let left_nested = r"\left(".repeat(delimiter_depth) + "x" + &r"\right)".repeat(delimiter_depth);
    assert!(render(&left_nested).is_none());

    let environment_depth = MAX_MATH_ROWS * 4;
    let matrices = r"\begin{matrix}".repeat(environment_depth)
        + "x"
        + &r"\end{matrix}".repeat(environment_depth);
    assert!(render(&matrices).is_none());
}
