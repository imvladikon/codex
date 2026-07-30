use super::*;
use pretty_assertions::assert_eq;

#[test]
fn normalizes_unbraced_fraction_tokens() {
    assert_eq!(
        normalize_strict_latex(r"\frac12"),
        Some(Cow::Owned(r"\frac{1}{2}".to_string())),
    );
    assert_eq!(
        normalize_strict_latex(r"\frac1x"),
        Some(Cow::Owned(r"\frac{1}{x}".to_string())),
    );
    assert_eq!(
        normalize_strict_latex(r"\frac\alpha\beta"),
        Some(Cow::Owned(r"\frac{\alpha}{\beta}".to_string())),
    );
    assert_eq!(
        normalize_strict_latex(r"\sqrt12"),
        Some(Cow::Owned(r"\sqrt{1}2".to_string())),
    );
    assert_eq!(
        normalize_strict_latex("x^12"),
        Some(Cow::Owned("x^{1}2".to_string())),
    );
    assert_eq!(
        normalize_strict_latex(r"\overset12"),
        Some(Cow::Owned(r"\overset{1}{2}".to_string())),
    );
    assert_eq!(
        normalize_strict_latex("x^1_2"),
        Some(Cow::Owned("x^{1}_{2}".to_string())),
    );
}

#[test]
fn accepts_supported_multiline_formulas() {
    for source in [
        concat!(
            "\\boxed{\n",
            "R_{\\mu\\nu}-\\frac12 Rg_{\\mu\\nu}+\\Lambda g_{\\mu\\nu}\n",
            "=\n",
            "\\frac{8\\pi G}{c^4}T_{\\mu\\nu}\n",
            "}\n",
        ),
        concat!(
            "\\sigma_{\\ell,m}[i,j]\n",
            "=\n",
            "\\operatorname{std}_s D_{\\ell,m,s}[i,j]\n",
        ),
    ] {
        assert_eq!(validate_structure(source), Some(()), "{source:?}");
        assert!(!has_ambiguous_barewords(source), "{source:?}");
        assert!(
            normalize_strict_latex(source).is_some(),
            "rejected {source:?}",
        );
    }
}

#[test]
fn normalizes_literal_slashes_in_explicit_latex() {
    assert_eq!(
        normalize_strict_latex("a/b"),
        Some(Cow::Owned(r"a\text{/}b".to_string())),
    );
    assert_eq!(
        normalize_strict_latex(r"\dot q=dq/dt"),
        Some(Cow::Owned(r"\dot {q}=dq\text{/}dt".to_string())),
    );
    assert_eq!(
        normalize_strict_latex(r"\text{rate/day}=dq/dt"),
        Some(Cow::Owned(r"\text{rate/day}=dq\text{/}dt".to_string(),)),
    );
    assert_eq!(
        normalize_strict_latex(r"\operatorname{a/b}"),
        Some(Cow::Owned(r"\operatorname{a\text{/}b}".to_string(),)),
    );
}

#[test]
fn normalizes_common_latex_aliases() {
    assert_eq!(
        normalize_strict_latex(
            r"\tfrac12+\widehat f+\Box+a^\dagger+\langle x\mid y\rangle+\lVert z\rVert"
        ),
        Some(Cow::Owned(
            r"\frac{1}{2}+\hat {f}+□+a^{†}+⟨ x |  y⟩+‖ z‖".to_string(),
        )),
    );
    assert_eq!(
        normalize_strict_latex(r"\bigl(x\bigr)\Longleftrightarrow y\longrightarrow z"),
        Some(Cow::Owned(r"(x)\Leftrightarrow y\to z".to_string())),
    );
    assert_eq!(
        normalize_strict_latex(r"\bigg|_0^\infty"),
        Some(Cow::Owned(r"|_{0}^{\infty}".to_string())),
    );
    assert_eq!(
        normalize_strict_latex(r"x\xrightarrow{d}y"),
        Some(Cow::Owned(r"x\overset{d}{\to}y".to_string())),
    );
}

#[test]
fn normalizes_double_vertical_bar_commands() {
    assert_eq!(
        normalize_strict_latex(r"\|x\|"),
        Some(Cow::Owned("‖x‖".to_string())),
    );
    assert_eq!(
        normalize_strict_latex(r"\left\|x\right\|"),
        Some(Cow::Owned(r"\left‖x\right‖".to_string())),
    );
    assert_eq!(
        normalize_strict_latex(r"\Vert x\vert"),
        Some(Cow::Owned("‖ x|".to_string())),
    );
}

#[test]
fn rejects_dependency_shortcuts_and_truncated_syntax() {
    for source in ["pi", "alpha+beta", "sqrt(x)"] {
        assert!(has_ambiguous_barewords(source), "{source:?}");
    }
    for source in [
        "x}}}",
        r"\frac{a",
        r"\sqrt[3]{x}",
        "x & y",
        r"\begin{matrix}a & b",
        r"\left(x",
        "x^",
        r"x \\ y",
        r"\begin{matrix}x\end{pmatrix}",
        r"\end{matrix}",
        r"\right)",
        r"\frac^12",
        r"\sqrt_x",
        r"\text(x)",
        r"\operatorname x",
        r"\overset1",
    ] {
        assert_eq!(normalize_strict_latex(source), None, "{source:?}");
    }
}

#[test]
fn accepts_matched_delimiters_environments_and_adjacent_scripts() {
    for source in [
        r"\left(x\right)",
        r"\begin{matrix}\begin{matrix}x\end{matrix}\end{matrix}",
        "x^1_2",
    ] {
        assert!(
            normalize_strict_latex(source).is_some(),
            "rejected {source:?}",
        );
    }
}
