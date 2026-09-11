use crate::models::{Piece, PlaceholderKind};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
#[error("{message} na linha {line}, coluna {column}: {context}")]
pub struct ScanError {
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub context: String,
}

const OPAQUE_ENVS: &[&str] = &[
    "equation",
    "equation*",
    "align",
    "align*",
    "alignat",
    "alignat*",
    "gather",
    "gather*",
    "multline",
    "multline*",
    "tikzpicture",
    "figure",
    "figure*",
    "table",
    "table*",
    "tabular",
    "tabular*",
    "verbatim",
    "verbatim*",
    "lstlisting",
    "minted",
    "thebibliography",
    "aligned",
    "array",
    "cases",
    "axis",
    "comment",
    "formula",
    "sagesilent",
];
const TEXT_ENVS: &[&str] = &[
    "document",
    "itemize",
    "enumerate",
    "description",
    "quote",
    "quotation",
    "center",
    "flushleft",
    "flushright",
    "minipage",
    "abstract",
    "proof",
    "theorem",
    "lemma",
    "proposition",
    "definition",
    "example",
    "remark",
    "exercise",
    "solution",
    "frame",
    "columns",
    "column",
    "dialogue",
    "explanation",
    "feedback",
    "hint",
    "image",
    "multipleChoice",
    "problem",
    "prompt",
    "question",
    "sectionOutcomes",
    "selectAll",
    "warning",
    "expandable",
    "freeResponse",
    "onlineOnly",
    "corollary",
    "observation",
];
const TEXT_ARGUMENT_COMMANDS: &[&str] = &[
    "emph",
    "textbf",
    "textit",
    "textrm",
    "textsf",
    "texttt",
    "textsc",
    "textnormal",
    "text",
    "choice",
    "wordChoice",
    "outcome",
    "section",
    "subsection",
    "subsubsection",
    "paragraph",
];

fn position(input: &str, at: usize) -> (usize, usize) {
    let before = &input[..at];
    let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
    let col = before
        .rsplit('\n')
        .next()
        .map_or(1, |s| s.chars().count() + 1);
    (line, col)
}
fn error(input: &str, at: usize, message: impl Into<String>) -> ScanError {
    let (line, column) = position(input, at);
    ScanError {
        message: message.into(),
        line,
        column,
        context: input[at..].chars().take(40).collect(),
    }
}
fn starts(input: &str, at: usize, needle: &str) -> bool {
    input[at..].starts_with(needle)
}
fn next_char(input: &str, at: usize) -> Option<(char, usize)> {
    input[at..].chars().next().map(|c| (c, at + c.len_utf8()))
}
fn escaped(input: &str, at: usize) -> bool {
    input[..at]
        .bytes()
        .rev()
        .take_while(|b| *b == b'\\')
        .count()
        % 2
        == 1
}
fn balanced(input: &str, open_at: usize, open: char, close: char) -> Result<usize, ScanError> {
    let mut depth = 0usize;
    let mut at = open_at;
    while let Some((c, next)) = next_char(input, at) {
        if c == open && !escaped(input, at) {
            depth += 1;
        }
        if c == close && !escaped(input, at) {
            depth -= 1;
            if depth == 0 {
                return Ok(next);
            }
        }
        at = next;
    }
    Err(error(
        input,
        open_at,
        format!("delimitador `{open}` não fechado"),
    ))
}
fn command_end(input: &str, at: usize) -> Result<usize, ScanError> {
    let (_, mut end) = next_char(input, at).expect("backslash exists");
    if let Some((c, n)) = next_char(input, end) {
        if c.is_ascii_alphabetic() || c == '@' {
            end = n;
            while let Some((ch, n2)) = next_char(input, end) {
                if ch.is_ascii_alphabetic() || ch == '@' || ch == '*' {
                    end = n2;
                } else {
                    break;
                }
            }
        } else {
            end = n;
        }
    }
    let mut cursor = end;
    loop {
        let mut after_ws = cursor;
        while let Some((c, n)) = next_char(input, after_ws) {
            if c.is_whitespace() {
                after_ws = n;
            } else {
                break;
            }
        }
        let Some((c, _)) = next_char(input, after_ws) else {
            break;
        };
        if c == '[' {
            cursor = balanced(input, after_ws, '[', ']')?;
        } else if c == '{' {
            cursor = balanced(input, after_ws, '{', '}')?;
        } else {
            break;
        }
        // Whitespace only belongs to the command when it actually introduced an argument.
        if after_ws != cursor { /* cursor already includes it via the slice */ }
    }
    Ok(cursor)
}
fn env_name(input: &str, at: usize) -> Option<String> {
    let end = command_end(input, at).ok()?;
    let s = &input[at..end];
    let prefix = if s.starts_with("\\begin") {
        "\\begin"
    } else if s.starts_with("\\end") {
        "\\end"
    } else {
        return None;
    };
    let rest = &s[prefix.len()..];
    let a = rest.find('{')? + 1;
    let b = rest[a..].find('}')? + a;
    Some(rest[a..b].to_string())
}
fn find_environment_end(input: &str, begin_at: usize, name: &str) -> Result<usize, ScanError> {
    let mut at = command_end(input, begin_at)?;
    let mut depth = 1usize;
    while at < input.len() {
        let Some(found) = input[at..].find("\\") else {
            break;
        };
        let p = at + found;
        if starts(input, p, "\\begin") && env_name(input, p).as_deref() == Some(name) {
            depth += 1;
        }
        if starts(input, p, "\\end") && env_name(input, p).as_deref() == Some(name) {
            depth -= 1;
            let end = command_end(input, p)?;
            if depth == 0 {
                return Ok(end);
            }
            at = end;
            continue;
        }
        at = p + 1;
    }
    Err(error(
        input,
        begin_at,
        format!("ambiente `{name}` não fechado"),
    ))
}
fn push_prose(out: &mut Vec<Piece>, text: &str) {
    if text.is_empty() {
        return;
    }
    match out.last_mut() {
        Some(Piece::Prose(previous)) => previous.push_str(text),
        _ => out.push(Piece::Prose(text.to_owned())),
    }
}

/// Splits the human-facing label of common link commands from their protected
/// URL and syntax. The label is scanned recursively, so formulas inside it
/// remain protected too.
fn link_end(input: &str, at: usize, out: &mut Vec<Piece>) -> Result<Option<usize>, ScanError> {
    let (prefix_end, label_start, label_end, url_end) = if starts(input, at, "\\link[") {
        let label_start = at + "\\link[".len();
        let label_end = balanced(input, label_start - 1, '[', ']')?;
        let url_start = label_end;
        if !starts(input, url_start, "{") {
            return Ok(None);
        }
        (
            label_start,
            label_start,
            label_end - 1,
            balanced(input, url_start, '{', '}')?,
        )
    } else if starts(input, at, "\\href{") {
        let url_end = balanced(input, at + "\\href".len(), '{', '}')?;
        if !starts(input, url_end, "{") {
            return Ok(None);
        }
        let label_start = url_end + 1;
        let label_end = balanced(input, url_end, '{', '}')? - 1;
        (
            label_start,
            label_start,
            label_end,
            balanced(input, url_end, '{', '}')?,
        )
    } else {
        return Ok(None);
    };
    out.push(Piece::Protected {
        kind: PlaceholderKind::Command,
        original: input[at..prefix_end].to_owned(),
    });
    out.extend(scan(&input[label_start..label_end])?);
    out.push(Piece::Protected {
        kind: PlaceholderKind::Command,
        original: input[label_end..url_end].to_owned(),
    });
    Ok(Some(url_end))
}

fn text_argument_end(
    input: &str,
    at: usize,
    commands: &[&str],
    out: &mut Vec<Piece>,
) -> Result<Option<usize>, ScanError> {
    let Some(command) = commands.iter().find(|command| {
        let name_end = at + 1 + command.len();
        starts(input, at, &format!("\\{command}"))
            && next_char(input, name_end)
                .is_none_or(|(character, _)| !(character.is_ascii_alphabetic() || character == '@'))
    }) else {
        return Ok(None);
    };
    let mut cursor = at + 1 + command.len();
    if starts(input, cursor, "*") {
        cursor += 1;
    }
    loop {
        while let Some((character, next)) = next_char(input, cursor) {
            if character.is_whitespace() {
                cursor = next;
            } else {
                break;
            }
        }
        if starts(input, cursor, "[") {
            cursor = balanced(input, cursor, '[', ']')?;
        } else {
            break;
        }
    }
    if !starts(input, cursor, "{") {
        return Ok(None);
    }
    let end = balanced(input, cursor, '{', '}')?;
    out.push(Piece::Protected {
        kind: PlaceholderKind::Command,
        original: input[at..cursor + 1].to_owned(),
    });
    out.extend(scan(&input[cursor + 1..end - 1])?);
    out.push(Piece::Protected {
        kind: PlaceholderKind::Command,
        original: input[end - 1..end].to_owned(),
    });
    Ok(Some(end))
}

/// Conservative scanner: all LaTex syntax is protected; only plain body text is editable.
pub fn scan(input: &str) -> Result<Vec<Piece>, ScanError> {
    if !input.contains("\\begin{document}") && is_standalone_preamble(input) {
        return Ok(vec![Piece::Protected {
            kind: PlaceholderKind::Preamble,
            original: input.to_owned(),
        }]);
    }
    let mut out = Vec::new();
    let mut at = 0usize;
    let mut prose_start = 0usize;
    let mut in_body = !input.contains("\\begin{document}");
    while at < input.len() {
        if !in_body {
            if starts(input, at, "\\title") || starts(input, at, "\\subtitle") {
                out.push(Piece::Protected {
                    kind: PlaceholderKind::Preamble,
                    original: input[prose_start..at].to_owned(),
                });
                if let Some(end) = text_argument_end(input, at, &["title", "subtitle"], &mut out)? {
                    at = end;
                    prose_start = end;
                    continue;
                }
            }
            if starts(input, at, "\\begin{document}") {
                out.push(Piece::Protected {
                    kind: PlaceholderKind::Preamble,
                    original: input[prose_start..at].to_owned(),
                });
                let end = command_end(input, at)?;
                out.push(Piece::Protected {
                    kind: PlaceholderKind::Command,
                    original: input[at..end].to_owned(),
                });
                at = end;
                prose_start = end;
                in_body = true;
                continue;
            }
            at = next_char(input, at).unwrap().1;
            continue;
        }
        if starts(input, at, "\\end{document}") {
            push_prose(&mut out, &input[prose_start..at]);
            out.push(Piece::Protected {
                kind: PlaceholderKind::Command,
                original: input[at..].to_owned(),
            });
            return Ok(out);
        }
        let c = next_char(input, at).unwrap().0;
        if c == '%' && !escaped(input, at) {
            push_prose(&mut out, &input[prose_start..at]);
            let end = input[at..].find('\n').map_or(input.len(), |n| at + n);
            out.push(Piece::Protected {
                kind: PlaceholderKind::Comment,
                original: input[at..end].to_owned(),
            });
            at = end;
            prose_start = end;
            continue;
        }
        if c == '$' && !escaped(input, at) {
            push_prose(&mut out, &input[prose_start..at]);
            let display = starts(input, at, "$$");
            let from = at + if display { 2 } else { 1 };
            let delimiter = if display { "$$" } else { "$" };
            let mut seek = from;
            let mut end = None;
            while let Some(n) = input[seek..].find(delimiter) {
                let p = seek + n;
                if !escaped(input, p) {
                    end = Some(p + delimiter.len());
                    break;
                }
                seek = p + 1;
            }
            let end = end.ok_or_else(|| error(input, at, "matemática `$` não fechada"))?;
            out.push(Piece::Protected {
                kind: PlaceholderKind::Math,
                original: input[at..end].to_owned(),
            });
            at = end;
            prose_start = end;
            continue;
        }
        if starts(input, at, "\\(") || starts(input, at, "\\[") {
            push_prose(&mut out, &input[prose_start..at]);
            let closing = if starts(input, at, "\\(") {
                "\\)"
            } else {
                "\\]"
            };
            let end = input[at + 2..]
                .find(closing)
                .map(|n| at + 2 + n + 2)
                .ok_or_else(|| error(input, at, "matemática delimitada não fechada"))?;
            out.push(Piece::Protected {
                kind: PlaceholderKind::Math,
                original: input[at..end].to_owned(),
            });
            at = end;
            prose_start = end;
            continue;
        }
        if c == '\\' {
            push_prose(&mut out, &input[prose_start..at]);
            if let Some(end) = link_end(input, at, &mut out)? {
                at = end;
                prose_start = end;
                continue;
            }
            if let Some(end) = text_argument_end(input, at, TEXT_ARGUMENT_COMMANDS, &mut out)? {
                at = end;
                prose_start = end;
                continue;
            }
            if starts(input, at, "\\begin") {
                let name = env_name(input, at)
                    .ok_or_else(|| error(input, at, "nome de ambiente inválido"))?;
                if OPAQUE_ENVS.contains(&name.as_str()) {
                    let end = find_environment_end(input, at, &name)?;
                    out.push(Piece::Protected {
                        kind: PlaceholderKind::Environment,
                        original: input[at..end].to_owned(),
                    });
                    at = end;
                    prose_start = end;
                    continue;
                }
                if !TEXT_ENVS.contains(&name.as_str()) {
                    return Err(error(input, at, format!("ambiente desconhecido `{name}`")));
                }
            }
            let end = command_end(input, at)?;
            out.push(Piece::Protected {
                kind: PlaceholderKind::Command,
                original: input[at..end].to_owned(),
            });
            at = end;
            prose_start = end;
            continue;
        }
        at = next_char(input, at).unwrap().1;
    }
    if in_body {
        push_prose(&mut out, &input[prose_start..]);
    }
    Ok(out)
}

fn is_standalone_preamble(input: &str) -> bool {
    let first_content = input.lines().find_map(|line| {
        let line = line.trim_start();
        (!line.is_empty() && !line.starts_with('%')).then_some(line)
    });
    matches!(
        first_content,
        Some(line)
            if line.starts_with("\\documentclass")
                || line.starts_with("\\usepackage")
                || line.starts_with("\\newcommand")
                || line.starts_with("\\renewcommand")
                || line.starts_with("\\def")
                || line.starts_with("\\input")
                || line.starts_with("\\RequirePackage")
    )
}
