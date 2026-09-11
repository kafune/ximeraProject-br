use crate::models::{ParsedSegment, Piece, Placeholder, PlaceholderKind};

fn kind_name(kind: &PlaceholderKind) -> &'static str {
    match kind {
        PlaceholderKind::Math => "MATH",
        PlaceholderKind::Command => "CMD",
        PlaceholderKind::Comment => "COMMENT",
        PlaceholderKind::Environment => "ENV",
        PlaceholderKind::Preamble => "PREAMBLE",
        PlaceholderKind::Delimiter => "DELIM",
    }
}
fn has_editable_text(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes.get(at..at + 2) == Some(b"{{") {
            let mut cursor = at + 2;
            let name_start = cursor;
            while cursor < bytes.len() && bytes[cursor].is_ascii_uppercase() {
                cursor += 1;
            }
            if cursor > name_start && bytes.get(cursor) == Some(&b'_') {
                cursor += 1;
                let number_start = cursor;
                while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
                    cursor += 1;
                }
                if cursor > number_start && bytes.get(cursor..cursor + 2) == Some(b"}}") {
                    at = cursor + 2;
                    continue;
                }
            }
        }
        let character = text[at..].chars().next().expect("valid UTF-8");
        if !character.is_whitespace() && character != '{' && character != '}' {
            return true;
        }
        at += character.len_utf8();
    }
    false
}

/// Turns pieces into contiguous editable ranges. Boundaries prefer paragraphs and stay below ~1200 chars.
pub fn segment(pieces: &[Piece]) -> Vec<ParsedSegment> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut placeholders = Vec::new();
    let mut start = 0usize;
    let mut offset = 0usize;
    let mut counter = 0usize;
    let flush = |result: &mut Vec<ParsedSegment>,
                 current: &mut String,
                 placeholders: &mut Vec<Placeholder>,
                 start: &mut usize,
                 offset: usize| {
        if has_editable_text(current) {
            result.push(ParsedSegment {
                original: std::mem::take(current),
                placeholders: std::mem::take(placeholders),
                start: *start,
                end: offset,
            });
            *start = offset;
        }
    };
    for piece in pieces {
        match piece {
            Piece::Prose(text) => {
                // Preserve paragraph separators in the preceding segment, keeping exact source ranges.
                let mut rest = text.as_str();
                while let Some(pos) = rest.find("\n\n") {
                    let part = &rest[..pos + 2];
                    current.push_str(part);
                    offset += part.len();
                    flush(
                        &mut result,
                        &mut current,
                        &mut placeholders,
                        &mut start,
                        offset,
                    );
                    rest = &rest[pos + 2..];
                }
                current.push_str(rest);
                offset += rest.len();
                // `rest` is a pure-prose piece, so a sentence boundary inside it has an
                // unambiguous byte offset even when protected tokens precede it.
                if current.len() > 1200 {
                    let prefix_len = current.len() - rest.len();
                    let min = 800usize.saturating_sub(prefix_len);
                    let max = 1200usize.saturating_sub(prefix_len);
                    let sentence_end = rest.char_indices().find_map(|(i, c)| {
                        let end = i + c.len_utf8();
                        (end >= min && end <= max && matches!(c, '.' | '!' | '?' | ';'))
                            .then_some(end)
                    });
                    if let Some(cut_in_rest) = sentence_end {
                        let tail = current.split_off(prefix_len + cut_in_rest);
                        let boundary = offset - rest.len() + cut_in_rest;
                        flush(
                            &mut result,
                            &mut current,
                            &mut placeholders,
                            &mut start,
                            boundary,
                        );
                        current = tail;
                        start = boundary;
                    }
                }
            }
            Piece::Protected { kind, original } => {
                // Structural material before the first editable character belongs to the
                // file's unchanged source range. Keeping it out of the segment avoids a
                // translator opening on a wall of commands while preserving it on export.
                if !has_editable_text(&current) {
                    current.clear();
                    placeholders.clear();
                    offset += original.len();
                    start = offset;
                    continue;
                }
                counter += 1;
                let token = format!("{{{{{}_{}}}}}", kind_name(kind), counter);
                current.push_str(&token);
                placeholders.push(Placeholder {
                    token,
                    original: original.clone(),
                    kind: kind.clone(),
                });
                offset += original.len();
            }
        }
    }
    flush(
        &mut result,
        &mut current,
        &mut placeholders,
        &mut start,
        offset,
    );
    result
}

pub fn reconstruct(text: &str, placeholders: &[Placeholder]) -> Result<String, String> {
    let mut result = text.to_owned();
    for p in placeholders {
        let count = result.matches(&p.token).count();
        if count != 1 {
            return Err(format!("placeholder `{}` aparece {count} vezes", p.token));
        }
        result = result.replacen(&p.token, &p.original, 1);
    }
    if has_token_like_placeholder(&result) {
        return Err("placeholder desconhecido ou alterado".into());
    }
    Ok(result)
}

fn has_token_like_placeholder(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut at = 0;
    while at + 2 <= bytes.len() {
        let Some(relative) = text[at..].find("{{") else {
            return false;
        };
        let mut cursor = at + relative + 2;
        let name_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_uppercase() {
            cursor += 1;
        }
        if cursor == name_start || bytes.get(cursor) != Some(&b'_') {
            at = cursor;
            continue;
        }
        cursor += 1;
        let number_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if cursor > number_start && bytes.get(cursor..cursor + 2) == Some(b"}}") {
            return true;
        }
        at = cursor;
    }
    false
}
