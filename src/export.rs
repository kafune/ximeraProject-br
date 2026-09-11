use crate::{db, models::Segment, parser};
use anyhow::{bail, Context, Result};
use std::{fs, path::Path};
use walkdir::WalkDir;
#[derive(Default, Debug)]
pub struct ExportReport {
    pub files: usize,
    pub completed: usize,
    pub pending: usize,
    /// Pending segments with an unfinished draft; they are exported as the original.
    pub drafts: usize,
}
pub fn export_tree(
    db_path: &Path,
    source: &Path,
    output: &Path,
    require_complete: bool,
) -> Result<ExportReport> {
    let conn = db::open(db_path)?;
    if output.exists() {
        bail!("diretório de saída já existe: {}", output.display())
    };
    fs::create_dir_all(output)?;
    for entry in WalkDir::new(source).into_iter().filter_map(Result::ok) {
        let rel = entry.path().strip_prefix(source)?;
        let dest = output.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(dest)?
        } else {
            fs::copy(entry.path(), dest)?;
        }
    }
    let mut report = ExportReport::default();
    for file in db::file_records(&conn)? {
        let segs = db::segments_for_file(&conn, file.id)?;
        for seg in &segs {
            if seg.is_complete() {
                report.completed += 1;
                continue;
            }
            report.pending += 1;
            if !seg.translated.trim().is_empty() {
                report.drafts += 1;
            }
            if require_complete {
                bail!("segmento pendente {} em {}", seg.id, file.path)
            };
        }
        let result = translated_source(&file.source, &segs).map_err(anyhow::Error::msg)?;
        let target = output.join(&file.path);
        if let Some(p) = target.parent() {
            fs::create_dir_all(p)?
        };
        fs::write(target, result).with_context(|| format!("exportando {}", file.path))?;
        report.files += 1;
    }
    Ok(report)
}

/// Rebuilds a stored file with its completed translations. Drafts and pending
/// segments keep the original prose, so the export and the reader always agree.
pub fn translated_source(source: &str, segments: &[Segment]) -> Result<String, String> {
    let mut result = source.to_owned();
    for segment in segments.iter().rev() {
        let original = parser::reconstruct(&segment.original, &segment.placeholders)?;
        if result.get(segment.start..segment.end) != Some(original.as_str()) {
            return Err(format!(
                "fonte armazenada não corresponde ao segmento {}",
                segment.id
            ));
        }
        if segment.is_complete() {
            let translation = preserve_edge_whitespace(&segment.original, &segment.translated);
            let translated = parser::reconstruct(&translation, &segment.placeholders)?;
            result.replace_range(segment.start..segment.end, &translated);
        }
    }
    Ok(result)
}

/// Translators rarely retype the whitespace around a segment, yet it is what
/// separates the text from a preceding `\item` or from the next paragraph.
fn preserve_edge_whitespace(original: &str, translation: &str) -> String {
    let leading = original.len() - original.trim_start().len();
    let trailing = original.len() - original.trim_end().len();
    let mut result = String::new();
    if leading > 0 && translation.len() == translation.trim_start().len() {
        result.push_str(&original[..leading]);
    }
    result.push_str(translation);
    if trailing > 0 && result.len() == result.trim_end().len() {
        result.push_str(&original[original.len() - trailing..]);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Placeholder, PlaceholderKind};

    #[test]
    fn keeps_unfinished_prose_and_document_structure() {
        let source = "Texto \\textbf{mais texto}";
        let segment = Segment {
            id: 1,
            file_id: 1,
            ordem: 0,
            original: "Texto {{CMD_1}}mais texto{{DELIM_2}}".into(),
            translated: "Rascunho {{CMD_1}}inacabado{{DELIM_2}}".into(),
            status: "em_progresso".into(),
            placeholders: vec![
                Placeholder {
                    token: "{{CMD_1}}".into(),
                    original: "\\textbf{".into(),
                    kind: PlaceholderKind::Command,
                },
                Placeholder {
                    token: "{{DELIM_2}}".into(),
                    original: "}".into(),
                    kind: PlaceholderKind::Delimiter,
                },
            ],
            start: 0,
            end: source.len(),
        };
        assert_eq!(translated_source(source, &[segment]).unwrap(), source);
    }
}
