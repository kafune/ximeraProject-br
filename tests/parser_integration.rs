use traduz::parser::{reconstruct, scan, segment};
fn roundtrip(source: &str) {
    let pieces = scan(source).unwrap();
    let segments = segment(&pieces);
    for s in segments {
        assert_eq!(
            reconstruct(&s.original, &s.placeholders).unwrap(),
            &source[s.start..s.end]
        );
    }
}
#[test]
fn protects_latex_and_preserves_unicode() {
    roundtrip("\\documentclass{article}\n\\begin{document}\nOlá \\emph{mundo}, $x^2$ e \\[ y=2 \\]. % nota\nTexto.\n\\end{document}\n");
}
#[test]
fn preserves_opaque_environments() {
    roundtrip("Antes.\\begin{align}a&=b\\\\ c&=d\\end{align}Depois.");
}
#[test]
fn rejects_unclosed_math() {
    assert!(scan("Antes $x").is_err());
}
#[test]
fn rejects_unknown_environment() {
    assert!(scan("\\begin{estranho}x\\end{estranho}").is_err());
}

#[test]
fn does_not_create_segments_formed_only_by_placeholders() {
    let source = "\\documentclass{article}\n\\begin{document}\n% comentário\n\\setcounter{page}{1}\n\\end{document}\n";
    assert!(segment(&scan(source).unwrap()).is_empty());
}

#[test]
fn standalone_preamble_is_not_editable_content() {
    let source = "% packages\n\\newcommand{\\RR}{\\mathbb R}\n\\usepackage{tikz}\n";
    assert!(segment(&scan(source).unwrap()).is_empty());
}

#[test]
fn segment_begins_with_prose_after_leading_structure() {
    let source = "\\documentclass{article}\n\\begin{document}\n\\begin{problem}\nTranslate this sentence.\n\\end{problem}\n\\end{document}\n";
    let segments = segment(&scan(source).unwrap());
    assert_eq!(segments.len(), 1);
    assert!(segments[0]
        .original
        .trim_start()
        .starts_with("Translate this sentence."));
    assert_eq!(
        reconstruct(&segments[0].original, &segments[0].placeholders).unwrap(),
        &source[segments[0].start..segments[0].end]
    );
}

#[test]
fn link_label_stays_editable_while_url_stays_protected() {
    let source = "It is actively \\link[constructed]{https://example.test/article} by learners.";
    let segment = segment(&scan(source).unwrap()).pop().unwrap();
    assert!(segment.original.contains("constructed"));
    assert!(!segment
        .placeholders
        .iter()
        .any(|placeholder| placeholder.original.contains("constructed")));
    assert_eq!(
        reconstruct(&segment.original, &segment.placeholders).unwrap(),
        source
    );
}

#[test]
fn formatting_and_choices_keep_their_words_editable() {
    let source = "A \\textbf{bold $x$} choice: \\wordChoice{\\choice[correct]{yes}\\choice{no}}.";
    let segment = segment(&scan(source).unwrap()).pop().unwrap();
    for word in ["bold", "yes", "no"] {
        assert!(segment.original.contains(word));
        assert!(!segment
            .placeholders
            .iter()
            .any(|placeholder| placeholder.original.contains(word)));
    }
    assert!(segment
        .placeholders
        .iter()
        .any(|placeholder| placeholder.original == "$x$"));
    assert_eq!(
        reconstruct(&segment.original, &segment.placeholders).unwrap(),
        source
    );
}
