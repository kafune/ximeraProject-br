//! Rendering bridge for Ximera activities.
//!
//! Ximera's presentation is produced by its `ximeraLatex` TeX4ht rules, not
//! by a hand-written Markdown renderer.  This module invokes that same
//! renderer in its official, network-isolated Docker image and caches the
//! resulting activity fragment.  Only completed Traduz segments are injected;
//! drafts and pending prose keep the original text, exactly as in the export.

use crate::{
    export,
    models::{FileRecord, Placeholder, Segment},
    parser,
};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Component, Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const DEFAULT_SOURCE_ROOT: &str = "courses/ximera-calculus-1";
const DEFAULT_CACHE_ROOT: &str = "data/ximera-render";
const DEFAULT_IMAGE: &str = "ghcr.io/ximeraproject/ximeralatex:v2.7.2";

#[derive(Debug, Clone, Default)]
pub struct ActivityMetadata {
    pub title: Option<String>,
    pub summary: Option<String>,
}

pub fn metadata(translation: &str, placeholders: &[Placeholder]) -> ActivityMetadata {
    let source = parser::reconstruct(translation, placeholders).ok();
    ActivityMetadata {
        // The segmenter starts an editable range *inside* `\\title{...}`, so
        // the opening command is deliberately not in the stored segment.  The
        // first prose piece is therefore the title; the compiler still sees
        // the original opening command when rendering the full source.
        title: source
            .as_deref()
            .and_then(|value| command_argument(value, "title"))
            .and_then(plain_text)
            .or_else(|| leading_prose(translation).and_then(plain_text)),
        summary: source
            .as_deref()
            .and_then(|value| environment_body(value, "abstract"))
            .and_then(plain_text)
            .or_else(|| abstract_prose(translation, placeholders).and_then(plain_text)),
    }
}

pub fn render(file: &FileRecord, segments: &[Segment]) -> Result<String, String> {
    ensure_safe_translations(segments)?;
    let source = export::translated_source(&file.source, segments)?;
    let signature = signature(file, &source);
    let cache_root = env::var_os("TRADUZ_XIMERA_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CACHE_ROOT));
    fs::create_dir_all(&cache_root).map_err(io_error("criando cache do Ximera"))?;
    let cache_file = cache_root.join(format!("{}-{signature}.html", file.id));
    if let Ok(cached) = fs::read_to_string(&cache_file) {
        return Ok(cached);
    }

    let source_root = env::var_os("TRADUZ_XIMERA_SOURCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SOURCE_ROOT))
        .canonicalize()
        .map_err(io_error("abrindo a fonte do curso Ximera"))?;
    if !source_root.join(".git").is_dir() {
        return Err(format!(
            "a fonte Ximera `{}` não é um repositório Git",
            source_root.display()
        ));
    }
    let relative = safe_relative_path(&file.path)?;
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));
    let filename = relative
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "nome de atividade inválido".to_string())?;

    let work_root = cache_root.join(format!(
        ".work-{}-{}-{}",
        file.id,
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "relógio do sistema inválido".to_string())?
            .as_nanos()
    ));
    let rendered = (|| {
        let checkout = work_root.join("course");
        let clone = Command::new("git")
            .args(["clone", "--shared", "--quiet"])
            .arg(&source_root)
            .arg(&checkout)
            .output()
            .map_err(io_error("criando cópia temporária do curso"))?;
        if !clone.status.success() {
            return Err(command_error("git clone", &clone.stderr));
        }
        // Docker bind mounts require an absolute source path.  This also keeps
        // the later image traversal check anchored at the checked-out course.
        let checkout = checkout
            .canonicalize()
            .map_err(io_error("resolvendo a cópia temporária do curso"))?;

        let activity = checkout.join(&relative);
        fs::write(&activity, source)
            .map_err(io_error("gravando atividade traduzida temporária"))?;

        let image = env::var("TRADUZ_XIMERA_IMAGE").unwrap_or_else(|_| DEFAULT_IMAGE.to_owned());
        let mount = format!("type=bind,source={},target=/code", checkout.display());
        let workdir = if parent.as_os_str().is_empty() {
            "/code".to_owned()
        } else {
            format!("/code/{}", parent.display())
        };
        let compile = Command::new("timeout")
            .arg("90s")
            .arg("docker")
            .args(["run", "--rm", "--network", "none", "--mount"])
            .arg(mount)
            .args(["-w", &workdir])
            .arg(image)
            .args(["xmlatex", "texHtml", filename])
            .output()
            .map_err(io_error("executando o renderizador oficial do Ximera"))?;
        if !compile.status.success() {
            return Err(command_error("ximeraLatex", &compile.stderr));
        }

        let html = fs::read_to_string(activity.with_extension("html"))
            .map_err(io_error("lendo o HTML gerado pelo Ximera"))?;
        let body = activity_html(&html)?;
        inline_activity_images(&body, &activity, &checkout)
    })();
    let _ = fs::remove_dir_all(&work_root);

    let rendered = rendered?;
    fs::write(&cache_file, &rendered).map_err(io_error("gravando cache do Ximera"))?;
    Ok(rendered)
}

/// User-entered backslashes would be executable TeX when the official compiler
/// runs. Commands required by the document already live in protected tokens,
/// so reject new ones before entering the isolated compiler.
fn ensure_safe_translations(segments: &[Segment]) -> Result<(), String> {
    for segment in segments {
        if segment.is_complete() && segment.translated.contains('\\') {
            return Err(format!(
                "segmento {} contém um comando TeX fora dos tokens protegidos",
                segment.id
            ));
        }
    }
    Ok(())
}

fn signature(file: &FileRecord, source: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"ximera-render-v3");
    hasher.update(file.id.to_le_bytes());
    hasher.update(source.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn safe_relative_path(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
    {
        return Err("caminho de atividade fora da fonte do curso".to_string());
    }
    Ok(path.to_owned())
}

fn activity_html(html: &str) -> Result<String, String> {
    let body_start = html
        .find("<body")
        .and_then(|start| html[start..].find('>').map(|end| start + end + 1))
        .ok_or_else(|| "HTML do Ximera não contém body".to_string())?;
    let body_end = html[body_start..]
        .find("</body>")
        .map(|end| body_start + end)
        .unwrap_or(html.len());
    Ok(remove_scripts(&html[body_start..body_end]))
}

fn remove_scripts(input: &str) -> String {
    let mut result = input.to_owned();
    while let Some(start) = result.find("<script") {
        let Some(end) = result[start..].find("</script>") else {
            result.truncate(start);
            break;
        };
        result.replace_range(start..start + end + "</script>".len(), "");
    }
    result
}

fn inline_activity_images(html: &str, activity: &Path, checkout: &Path) -> Result<String, String> {
    let mut result = html.to_owned();
    for marker in ["src='", "src=\""] {
        let quote = marker.chars().last().expect("quote");
        let mut search = 0;
        while let Some(found) = result[search..].find(marker) {
            let value_start = search + found + marker.len();
            let Some(value_end) = result[value_start..].find(quote) else {
                break;
            };
            let value_end = value_start + value_end;
            let value = result[value_start..value_end].to_owned();
            search = value_end + 1;
            if value.contains("://") || value.starts_with("data:") || value.starts_with('/') {
                continue;
            }
            let candidate = activity
                .parent()
                .unwrap_or(checkout)
                .join(&value)
                .canonicalize();
            let Ok(candidate) = candidate else { continue };
            if !candidate.starts_with(checkout) || !candidate.is_file() {
                continue;
            }
            let bytes = fs::read(&candidate).map_err(io_error("lendo imagem do Ximera"))?;
            let data_url = format!("data:{};base64,{}", mime_type(&candidate), base64(&bytes));
            result.replace_range(value_start..value_end, &data_url);
            search = value_start + data_url.len() + 1;
        }
    }
    Ok(result)
}

fn mime_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = *chunk.get(1).unwrap_or(&0);
        let third = *chunk.get(2).unwrap_or(&0);
        output.push(ALPHABET[(first >> 2) as usize] as char);
        output.push(ALPHABET[(((first & 0b11) << 4) | (second >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[(((second & 0b1111) << 2) | (third >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            ALPHABET[(third & 0b11_1111) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn command_argument<'a>(source: &'a str, command: &str) -> Option<&'a str> {
    let needle = format!("\\{command}");
    let mut search = 0;
    while let Some(relative) = source[search..].find(&needle) {
        let command_start = search + relative;
        let mut cursor = command_start + needle.len();
        if source[cursor..]
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic())
        {
            search = cursor;
            continue;
        }
        cursor = skip_space(source, cursor);
        if source[cursor..].starts_with('[') {
            cursor = balanced_end(source, cursor, '[', ']')?;
            cursor = skip_space(source, cursor);
        }
        if source[cursor..].starts_with('{') {
            let end = balanced_end(source, cursor, '{', '}')?;
            return source.get(cursor + 1..end - 1);
        }
        search = cursor;
    }
    None
}

fn leading_prose(translation: &str) -> Option<&str> {
    let end = translation.find("{{").unwrap_or(translation.len());
    translation.get(..end)
}

fn abstract_prose<'a>(translation: &'a str, placeholders: &[Placeholder]) -> Option<&'a str> {
    let begin = placeholders
        .iter()
        .find(|placeholder| placeholder.original.contains("\\begin{abstract}"))?;
    let end = placeholders
        .iter()
        .find(|placeholder| placeholder.original.contains("\\end{abstract}"))?;
    let start = translation.find(&begin.token)? + begin.token.len();
    let end = translation[start..].find(&end.token)? + start;
    translation.get(start..end)
}

fn environment_body<'a>(source: &'a str, environment: &str) -> Option<&'a str> {
    let opening = format!("\\begin{{{environment}}}");
    let start = source.find(&opening)? + opening.len();
    let closing = format!("\\end{{{environment}}}");
    let end = source[start..].find(&closing)? + start;
    source.get(start..end)
}

fn skip_space(input: &str, mut at: usize) -> usize {
    while let Some(character) = input[at..].chars().next() {
        if !character.is_whitespace() {
            break;
        }
        at += character.len_utf8();
    }
    at
}

fn balanced_end(input: &str, start: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    let mut at = start;
    while let Some(character) = input[at..].chars().next() {
        if character == open {
            depth += 1;
        } else if character == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(at + character.len_utf8());
            }
        }
        at += character.len_utf8();
    }
    None
}

fn plain_text(value: &str) -> Option<String> {
    let mut output = String::new();
    let mut in_space = false;
    let mut at = 0;
    while let Some(character) = value[at..].chars().next() {
        if value[at..].starts_with("{{") {
            if let Some(end) = value[at + 2..].find("}}") {
                at += end + 4;
                continue;
            }
        }
        if character == '\\' {
            at += character.len_utf8();
            while let Some(command_character) = value[at..].chars().next() {
                if !(command_character.is_ascii_alphabetic()
                    || command_character == '@'
                    || command_character == '*')
                {
                    break;
                }
                at += command_character.len_utf8();
            }
            continue;
        } else if character == '{' || character == '}' {
        } else if character.is_whitespace() {
            if !in_space {
                output.push(' ');
                in_space = true;
            }
        } else {
            output.push(character);
            in_space = false;
        }
        at += character.len_utf8();
    }
    let output = output.trim();
    (!output.is_empty()).then(|| output.to_owned())
}

fn command_error(command: &str, stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    let details = stderr.lines().last().unwrap_or("sem detalhes");
    format!("{command} falhou: {details}")
}

fn io_error(action: &'static str) -> impl FnOnce(std::io::Error) -> String {
    move |error| format!("erro ao {action}: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Placeholder, PlaceholderKind};

    #[test]
    fn extracts_title_and_abstract_from_ximera_source() {
        let source = r"\title{Como usar o Ximera}\begin{document}\begin{abstract}Esse curso é construído com Ximera.\end{abstract}\maketitle";
        assert_eq!(
            metadata(source, &[]).title,
            Some("Como usar o Ximera".to_owned())
        );
        assert_eq!(
            metadata(source, &[]).summary,
            Some("Esse curso é construído com Ximera.".to_owned())
        );
    }

    #[test]
    fn extracts_metadata_when_the_title_opening_is_outside_the_segment() {
        let placeholders = vec![
            Placeholder {
                token: "{{CMD_1}}".into(),
                original: "}".into(),
                kind: PlaceholderKind::Command,
            },
            Placeholder {
                token: "{{PREAMBLE_2}}".into(),
                original: "\n\n".into(),
                kind: PlaceholderKind::Preamble,
            },
            Placeholder {
                token: "{{CMD_3}}".into(),
                original: "\\begin{document}".into(),
                kind: PlaceholderKind::Command,
            },
            Placeholder {
                token: "{{CMD_4}}".into(),
                original: "\\begin{abstract}".into(),
                kind: PlaceholderKind::Command,
            },
            Placeholder {
                token: "{{CMD_5}}".into(),
                original: "\\end{abstract}".into(),
                kind: PlaceholderKind::Command,
            },
            Placeholder {
                token: "{{CMD_6}}".into(),
                original: "\\maketitle".into(),
                kind: PlaceholderKind::Command,
            },
        ];
        let activity = metadata(
            "Como usar o Ximera{{CMD_1}}{{PREAMBLE_2}}{{CMD_3}}\n{{CMD_4}}Esse curso é construído com Ximera.{{CMD_5}}{{CMD_6}}",
            &placeholders,
        );
        assert_eq!(activity.title.as_deref(), Some("Como usar o Ximera"));
        assert_eq!(
            activity.summary.as_deref(),
            Some("Esse curso é construído com Ximera.")
        );
    }
}
