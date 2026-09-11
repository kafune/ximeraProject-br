use crate::{
    models::{PlaceholderKind, Segment},
    parser,
};

#[derive(Debug, Clone)]
pub struct ReadBlock {
    pub html: String,
}

pub fn completed_block(segment: &Segment) -> Result<Option<ReadBlock>, String> {
    if segment.status != "traduzido" || segment.translated.is_empty() {
        return Ok(None);
    }
    // Validation also guarantees every protected token has exactly one source.
    parser::reconstruct(&segment.translated, &segment.placeholders)?;
    let mut html = String::new();
    let mut cursor = 0;
    for placeholder in &segment.placeholders {
        let position = segment.translated[cursor..]
            .find(&placeholder.token)
            .ok_or_else(|| format!("placeholder ausente: {}", placeholder.token))?
            + cursor;
        html.push_str(&escape(&segment.translated[cursor..position]));
        match placeholder.kind {
            PlaceholderKind::Math => html.push_str(&math_html(&placeholder.original)),
            // Formatting and link syntax are intentionally omitted here. Their human-facing
            // labels are separate prose pieces and therefore remain visible in the reader.
            PlaceholderKind::Command
            | PlaceholderKind::Comment
            | PlaceholderKind::Environment
            | PlaceholderKind::Preamble
            | PlaceholderKind::Delimiter => {}
        }
        cursor = position + placeholder.token.len();
    }
    html.push_str(&escape(&segment.translated[cursor..]));
    Ok(Some(ReadBlock { html }))
}

fn math_html(source: &str) -> String {
    let source = source
        .strip_prefix("$$")
        .and_then(|value| value.strip_suffix("$$"))
        .or_else(|| {
            source
                .strip_prefix("$")
                .and_then(|value| value.strip_suffix("$"))
        })
        .or_else(|| {
            source
                .strip_prefix("\\(")
                .and_then(|value| value.strip_suffix("\\)"))
        })
        .or_else(|| {
            source
                .strip_prefix("\\[")
                .and_then(|value| value.strip_suffix("\\]"))
        })
        .unwrap_or(source);
    // Ximera emits inline TeX with these same delimiters.  MathJax on the reader
    // page turns it into typeset mathematics, while the escaped source remains safe.
    format!(
        "<span class=\"reader-math\">\\({}\\)</span>",
        escape(source)
    )
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Placeholder, PlaceholderKind};

    #[test]
    fn reader_escapes_translation_and_keeps_math_visible() {
        let segment = Segment {
            id: 1,
            file_id: 1,
            ordem: 0,
            original: "<x> {{MATH_1}}".into(),
            translated: "<traduzido> {{MATH_1}}".into(),
            status: "traduzido".into(),
            placeholders: vec![Placeholder {
                token: "{{MATH_1}}".into(),
                original: "$x^2$".into(),
                kind: PlaceholderKind::Math,
            }],
            start: 0,
            end: 0,
        };
        let html = completed_block(&segment).unwrap().unwrap().html;
        assert!(html.contains("&lt;traduzido&gt;"));
        assert!(html.contains("reader-math\">\\(x^2\\)"));
    }
}
