use crate::{db, parser};
use anyhow::{bail, Context, Result};
use std::{fs, path::Path};
use walkdir::WalkDir;
#[derive(Default, Debug)]
pub struct ExportReport {
    pub files: usize,
    pub completed: usize,
    pub pending: usize,
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
        let mut result = file.source.clone();
        let segs = db::segments_for_file(&conn, file.id)?;
        for seg in segs.iter().rev() {
            let translated = if seg.translated.is_empty() {
                report.pending += 1;
                if require_complete {
                    bail!("segmento pendente {} em {}", seg.id, file.path)
                };
                seg.original.clone()
            } else {
                report.completed += 1;
                seg.translated.clone()
            };
            let injected =
                parser::reconstruct(&translated, &seg.placeholders).map_err(anyhow::Error::msg)?;
            if result.get(seg.start..seg.end)
                != Some(
                    parser::reconstruct(&seg.original, &seg.placeholders)
                        .map_err(anyhow::Error::msg)?
                        .as_str(),
                )
            {
                bail!("fonte armazenada não corresponde ao segmento {}", seg.id)
            };
            result.replace_range(seg.start..seg.end, &injected);
        }
        let target = output.join(&file.path);
        if let Some(p) = target.parent() {
            fs::create_dir_all(p)?
        };
        fs::write(target, result).with_context(|| format!("exportando {}", file.path))?;
        report.files += 1;
    }
    Ok(report)
}
