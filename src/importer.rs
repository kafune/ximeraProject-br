use crate::{
    db,
    models::{ParsedSegment, Segment},
    parser,
};
use anyhow::{anyhow, bail, Context, Result};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use walkdir::WalkDir;

#[derive(Debug, Default)]
pub struct ImportReport {
    pub imported: usize,
    pub unchanged: usize,
    pub segments: usize,
    /// Translations moved from replaced files onto identical new segments.
    pub translations_kept: usize,
    /// Replaced files whose translations could not be paired; those remain
    /// only in the backup.
    pub translations_dropped: Vec<(String, usize)>,
    pub backup: Option<std::path::PathBuf>,
}

#[derive(Default)]
struct CourseOrder {
    chapters: BTreeMap<String, i64>,
    files: BTreeMap<String, usize>,
}

struct PlannedFile {
    relative: String,
    chapter: String,
    chapter_order: i64,
    file_order: i64,
    source: String,
    hash: String,
    existing: Option<db::ExistingFile>,
    /// Parsed only for new and changed files; unchanged files keep their segments.
    segments: Option<Vec<ParsedSegment>>,
}

impl PlannedFile {
    fn moves_existing(&self) -> bool {
        self.existing.as_ref().is_some_and(|existing| {
            existing.chapter != self.chapter
                || existing.chapter_order != self.chapter_order
                || existing.order != self.file_order
        })
    }
}

pub fn import_tree(
    db_path: &Path,
    course_name: &str,
    source: &Path,
    replace: bool,
) -> Result<ImportReport> {
    import_tree_with_manifest(db_path, course_name, source, replace, None)
}

pub fn import_tree_with_manifest(
    db_path: &Path,
    course_name: &str,
    source: &Path,
    replace: bool,
    manifest: Option<&Path>,
) -> Result<ImportReport> {
    let mut conn = db::open(db_path)?;
    let mut report = ImportReport::default();
    let mut files: Vec<_> = WalkDir::new(source)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|x| x == "tex"))
        .collect();
    let course_order = manifest.map_or(Ok(CourseOrder::default()), load_course_order)?;
    files.sort_by_key(|entry| {
        let relative = entry
            .path()
            .strip_prefix(source)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .replace('\\', "/");
        (
            course_order
                .files
                .get(&relative)
                .copied()
                .unwrap_or(usize::MAX),
            relative,
        )
    });
    let chapter_names: std::collections::BTreeSet<String> = files
        .iter()
        .map(|entry| {
            entry
                .path()
                .strip_prefix(source)
                .ok()
                .and_then(|p| p.components().next())
                .and_then(|p| p.as_os_str().to_str())
                .unwrap_or("Raiz")
                .to_string()
        })
        .collect();
    let mut chapters: BTreeMap<String, i64> = course_order.chapters;
    let mut next_chapter_order = chapters.values().max().map_or(0, |n| n + 1);
    for name in chapter_names {
        chapters.entry(name).or_insert_with(|| {
            let order = next_chapter_order;
            next_chapter_order += 1;
            order
        });
    }

    // Everything is read and validated before the first write, so a rejected
    // import leaves the database exactly as it was.
    let mut plan = Vec::new();
    let mut changed = Vec::new();
    let mut file_orders = BTreeMap::<String, i64>::new();
    for entry in files {
        let full = entry.path();
        let relative = full
            .strip_prefix(source)?
            .to_string_lossy()
            .replace('\\', "/");
        let chapter = Path::new(&relative)
            .components()
            .next()
            .and_then(|x| x.as_os_str().to_str())
            .unwrap_or("Raiz")
            .to_string();
        let chapter_order = chapters[&chapter];
        let next_file_order = file_orders.entry(chapter.clone()).or_insert(0);
        let file_order = *next_file_order;
        *next_file_order += 1;
        let source_text =
            fs::read_to_string(full).with_context(|| format!("{} não é UTF-8", full.display()))?;
        let hash = format!("{:x}", Sha256::digest(source_text.as_bytes()));
        let existing = db::existing_file(&conn, &relative)?;
        if let Some(existing) = &existing {
            if existing.course != course_name {
                bail!(
                    "{relative} já foi importado no curso `{}`; o caminho precisa ser único no banco",
                    existing.course
                );
            }
            if existing.hash != hash && !replace {
                changed.push(relative);
                continue;
            }
        }
        let segments = match &existing {
            Some(existing) if existing.hash == hash => None,
            _ => Some(parse_file(&relative, &source_text)?),
        };
        plan.push(PlannedFile {
            relative,
            chapter,
            chapter_order,
            file_order,
            source: source_text,
            hash,
            existing,
            segments,
        });
    }
    if !changed.is_empty() {
        let mut listed = changed
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        if changed.len() > 10 {
            listed.push_str(&format!(" e mais {}", changed.len() - 10));
        }
        bail!(
            "{} arquivo(s) mudaram desde a última importação: {listed}; repita com --replace após conferir (o banco não foi alterado)",
            changed.len()
        );
    }

    let replaces = plan
        .iter()
        .any(|file| file.existing.is_some() && file.segments.is_some());
    let moves = plan.iter().any(PlannedFile::moves_existing);
    if replaces || moves {
        report.backup = Some(backup(&conn, db_path)?);
    }
    let parks = moves || plan.iter().any(|file| file.existing.is_none());

    let tx = conn.transaction()?;
    let course = db::course_id(&tx, course_name)?;
    if parks {
        db::park_file_orders(&tx, course)?;
    }
    for file in plan {
        let chapter_id = db::chapter_id(&tx, course, &file.chapter, file.chapter_order)?;
        let Some(segments) = file.segments else {
            if parks {
                let existing = file
                    .existing
                    .as_ref()
                    .expect("only existing files skip parsing");
                db::place_file(&tx, existing.id, chapter_id, file.file_order)?;
            }
            report.unchanged += 1;
            continue;
        };
        let translations = match &file.existing {
            Some(existing) => {
                let previous = db::segments_for_file(&tx, existing.id)?;
                let carried = carry_translations(&previous, &segments);
                let kept = carried.iter().flatten().count();
                let dropped = previous
                    .iter()
                    .filter(|segment| !segment.translated.is_empty())
                    .count()
                    .saturating_sub(kept);
                report.translations_kept += kept;
                if dropped > 0 {
                    report
                        .translations_dropped
                        .push((file.relative.clone(), dropped));
                }
                db::delete_file(&tx, existing.id)?;
                carried
            }
            None => vec![None; segments.len()],
        };
        db::insert_file(
            &tx,
            db::NewFile {
                chapter_id,
                path: &file.relative,
                file_order: file.file_order,
                hash: &file.hash,
                source: &file.source,
                segments: &segments,
                translations: &translations,
            },
        )?;
        report.imported += 1;
        report.segments += segments.len();
    }
    if parks {
        db::unpark_file_orders(&tx, course)?;
    }
    tx.commit()?;
    Ok(report)
}

fn parse_file(relative: &str, source: &str) -> Result<Vec<ParsedSegment>> {
    let pieces = parser::scan(source).map_err(|error| anyhow!("{relative}: {error}"))?;
    let segments = parser::segment(&pieces);
    // Only sources containing editable prose have segment spans. Validate each span rather than assuming protected-only files.
    for s in &segments {
        let text = parser::reconstruct(&s.original, &s.placeholders).map_err(anyhow::Error::msg)?;
        if source.get(s.start..s.end) != Some(text.as_str()) {
            bail!("falha de reconstrução exata em {relative}");
        }
    }
    Ok(segments)
}

fn backup(conn: &Connection, db_path: &Path) -> Result<PathBuf> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let backup = db_path.with_extension(format!("before-reimport-{stamp}.sqlite3"));
    if backup.exists() {
        bail!(
            "backup de reimportação já existe: {}; mova-o ou renomeie-o antes de continuar",
            backup.display()
        );
    }
    conn.execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])?;
    Ok(backup)
}

/// Pairs a replaced file's translations with new segments whose LaTeX is
/// byte-identical and unique on both sides, renumbering their tokens.
fn carry_translations(
    previous: &[Segment],
    segments: &[ParsedSegment],
) -> Vec<Option<(String, String)>> {
    let previous_sources: Vec<Option<String>> = previous
        .iter()
        .map(|segment| parser::reconstruct(&segment.original, &segment.placeholders).ok())
        .collect();
    let sources: Vec<Option<String>> = segments
        .iter()
        .map(|segment| parser::reconstruct(&segment.original, &segment.placeholders).ok())
        .collect();
    let previous_counts = occurrences(&previous_sources);
    let counts = occurrences(&sources);
    segments
        .iter()
        .zip(&sources)
        .map(|(segment, source)| {
            let source = source.as_deref()?;
            if previous_counts.get(source) != Some(&1) || counts.get(source) != Some(&1) {
                return None;
            }
            let index = previous_sources
                .iter()
                .position(|candidate| candidate.as_deref() == Some(source))?;
            let old = &previous[index];
            if old.translated.is_empty() || old.placeholders.len() != segment.placeholders.len() {
                return None;
            }
            let tokens = old
                .placeholders
                .iter()
                .zip(&segment.placeholders)
                .map(|(before, after)| {
                    (before.original == after.original)
                        .then_some((before.token.as_str(), after.token.as_str()))
                })
                .collect::<Option<Vec<_>>>()?;
            let translated = rename_tokens(&old.translated, &tokens);
            parser::reconstruct(&translated, &segment.placeholders).ok()?;
            Some((translated, old.status.clone()))
        })
        .collect()
}

fn occurrences(sources: &[Option<String>]) -> HashMap<&str, usize> {
    let mut counts = HashMap::new();
    for source in sources.iter().flatten() {
        *counts.entry(source.as_str()).or_default() += 1;
    }
    counts
}

/// Renames every token in one pass, so `{{MATH_1}}` → `{{MATH_2}}` cannot
/// collide with a `{{MATH_2}}` that is itself being renamed.
fn rename_tokens(text: &str, tokens: &[(&str, &str)]) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(character) = rest.chars().next() {
        let token = if rest.starts_with("{{") {
            tokens.iter().find(|(before, _)| rest.starts_with(*before))
        } else {
            None
        };
        if let Some((before, after)) = token {
            result.push_str(after);
            rest = &rest[before.len()..];
        } else {
            result.push(character);
            rest = &rest[character.len_utf8()..];
        }
    }
    result
}

fn load_course_order(manifest: &Path) -> Result<CourseOrder> {
    let source = fs::read_to_string(manifest)
        .with_context(|| format!("manifesto {} não é UTF-8", manifest.display()))?;
    let mut order = CourseOrder::default();
    for line in source
        .lines()
        .map(str::trim_start)
        .filter(|line| !line.starts_with('%'))
    {
        let Some(activity) = line.strip_prefix("\\activity{") else {
            continue;
        };
        let Some(path) = activity.split('}').next().filter(|path| !path.is_empty()) else {
            continue;
        };
        let path = PathBuf::from(path).to_string_lossy().replace('\\', "/");
        let index = order.files.len();
        order.files.entry(path.clone()).or_insert(index);
        if let Some(chapter) = Path::new(&path)
            .components()
            .next()
            .and_then(|part| part.as_os_str().to_str())
        {
            let chapter_index = order.chapters.len() as i64;
            order
                .chapters
                .entry(chapter.to_owned())
                .or_insert(chapter_index);
        }
    }
    if order.files.is_empty() {
        bail!("manifesto {} não contém atividades", manifest.display());
    }
    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::rename_tokens;

    #[test]
    fn renames_tokens_without_chaining_replacements() {
        let renamed = rename_tokens(
            "{{MATH_1}} e {{MATH_2}}, não {{MATH_12}}",
            &[("{{MATH_1}}", "{{MATH_2}}"), ("{{MATH_2}}", "{{MATH_3}}")],
        );
        assert_eq!(renamed, "{{MATH_2}} e {{MATH_3}}, não {{MATH_12}}");
    }
}
