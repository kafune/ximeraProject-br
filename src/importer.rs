use crate::{db, parser};
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
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
    pub backup: Option<std::path::PathBuf>,
}

#[derive(Default)]
struct CourseOrder {
    chapters: BTreeMap<String, i64>,
    files: BTreeMap<String, usize>,
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
    let course = db::course_id(&conn, course_name)?;
    let mut report = ImportReport::default();
    let mut backed_up = false;
    if replace && manifest.is_some() {
        let existing_files: i64 = conn.query_row(
            "SELECT COUNT(*) FROM files WHERE chapter_id IN (SELECT id FROM chapters WHERE course_id=?1)",
            [course],
            |row| row.get(0),
        )?;
        if existing_files > 0 {
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
            let backup = db_path.with_extension(format!("before-reimport-{stamp}.sqlite3"));
            if backup.exists() {
                bail!(
                    "backup de reimportação já existe: {}; mova-o ou renomeie-o antes de continuar",
                    backup.display()
                );
            }
            conn.execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])?;
            conn.execute(
                "UPDATE files SET ordem=ordem+1000000 WHERE chapter_id IN (SELECT id FROM chapters WHERE course_id=?1)",
                [course],
            )?;
            report.backup = Some(backup);
            backed_up = true;
        }
    }
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
    let mut file_orders = std::collections::BTreeMap::<String, i64>::new();
    for entry in files {
        let full = entry.path();
        let relative = full
            .strip_prefix(source)?
            .to_string_lossy()
            .replace('\\', "/");
        let parent = Path::new(&relative)
            .components()
            .next()
            .and_then(|x| x.as_os_str().to_str())
            .unwrap_or("Raiz")
            .to_string();
        let file_order = *file_orders.entry(parent.clone()).or_insert(0);
        *file_orders.get_mut(&parent).expect("chapter was inserted") += 1;
        let source_text =
            fs::read_to_string(full).with_context(|| format!("{} não é UTF-8", full.display()))?;
        let hash = format!("{:x}", Sha256::digest(source_text.as_bytes()));
        if let Some(old) = db::existing_hash(&conn, &relative)? {
            if old == hash && !replace {
                report.unchanged += 1;
                continue;
            }
            if old != hash && !replace {
                bail!("{} mudou desde a última importação; repita com --replace após conferir (o banco não foi alterado)",relative);
            }
            if !backed_up {
                let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
                let backup = db_path.with_extension(format!("before-reimport-{stamp}.sqlite3"));
                if backup.exists() {
                    bail!("backup de reimportação já existe: {}; mova-o ou renomeie-o antes de continuar", backup.display());
                }
                conn.execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])?;
                report.backup = Some(backup);
                backed_up = true;
            }
        }
        let pieces = parser::scan(&source_text).map_err(anyhow::Error::msg)?;
        let segments = parser::segment(&pieces);
        let rebuilt = segments
            .iter()
            .map(|s| parser::reconstruct(&s.original, &s.placeholders))
            .collect::<Result<Vec<_>, _>>()
            .map_err(anyhow::Error::msg)?
            .join("");
        // Only sources containing editable prose have segment spans. Validate each span rather than assuming protected-only files.
        for s in &segments {
            let text =
                parser::reconstruct(&s.original, &s.placeholders).map_err(anyhow::Error::msg)?;
            if source_text.get(s.start..s.end) != Some(text.as_str()) {
                bail!("falha de reconstrução exata em {relative}");
            }
        }
        let _ = rebuilt;
        let chapter_order = chapters[&parent];
        db::replace_file(
            &mut conn,
            db::NewFile {
                course,
                chapter: &parent,
                chapter_order,
                path: &relative,
                file_order,
                hash: &hash,
                source: &source_text,
                segments: &segments,
            },
        )?;
        report.imported += 1;
        report.segments += segments.len();
    }
    Ok(report)
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
