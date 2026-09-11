use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::{tempdir, TempDir};
use traduz::{db, export, importer};

fn setup(files: &[(&str, &str)]) -> (TempDir, PathBuf, PathBuf) {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source");
    for (path, content) in files {
        let full = source.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, content).unwrap();
    }
    let database = dir.path().join("db.sqlite3");
    (dir, source, database)
}

fn segment_ids(database: &Path, path: &str) -> Vec<i64> {
    let conn = db::open(database).unwrap();
    let file = db::file_records(&conn)
        .unwrap()
        .into_iter()
        .find(|file| file.path == path)
        .unwrap();
    db::segments_for_file(&conn, file.id)
        .unwrap()
        .iter()
        .map(|segment| segment.id)
        .collect()
}

fn translate(database: &Path, id: i64, from: &str, to: &str) {
    let conn = db::open(database).unwrap();
    let segment = db::segment(&conn, id).unwrap().unwrap();
    db::save_segment(&conn, id, &segment.original.replace(from, to), true).unwrap();
}

fn file_order(database: &Path) -> Vec<String> {
    db::chapters(&db::open(database).unwrap())
        .unwrap()
        .into_iter()
        .flat_map(|(_, files)| files.into_iter().map(|(path, _)| path))
        .collect()
}

#[test]
fn replace_keeps_translations_of_unchanged_files() {
    let (_dir, source, database) = setup(&[("cap/a.tex", "Hello world.\n")]);
    importer::import_tree(&database, "C", &source, false).unwrap();
    let id = segment_ids(&database, "cap/a.tex")[0];
    translate(&database, id, "Hello world", "Olá mundo");

    let report = importer::import_tree(&database, "C", &source, true).unwrap();

    assert_eq!((report.imported, report.unchanged), (0, 1));
    assert!(report.backup.is_none());
    let segment = db::segment(&db::open(&database).unwrap(), id)
        .unwrap()
        .unwrap();
    assert_eq!(segment.translated, "Olá mundo.\n");
    assert_eq!(segment.status, "traduzido");
}

#[test]
fn replace_carries_translations_to_identical_segments_of_a_changed_file() {
    let (dir, source, database) = setup(&[(
        "cap/a.tex",
        "First $a$ paragraph.\n\nSecond $b$ paragraph.\n",
    )]);
    importer::import_tree(&database, "C", &source, false).unwrap();
    let ids = segment_ids(&database, "cap/a.tex");
    translate(&database, ids[0], "First", "Primeiro");
    translate(&database, ids[1], "Second", "Segundo");
    // The new paragraph shifts the number of every token after it.
    fs::write(
        source.join("cap/a.tex"),
        "New $c$ intro.\n\nFirst $a$ paragraph, edited.\n\nSecond $b$ paragraph.\n",
    )
    .unwrap();

    assert!(importer::import_tree(&database, "C", &source, false).is_err());
    let report = importer::import_tree(&database, "C", &source, true).unwrap();

    assert_eq!(report.translations_kept, 1);
    assert_eq!(
        report.translations_dropped,
        vec![("cap/a.tex".to_owned(), 1)]
    );
    assert!(report.backup.is_some());
    let out = dir.path().join("out");
    export::export_tree(&database, &source, &out, false).unwrap();
    assert_eq!(
        fs::read_to_string(out.join("cap/a.tex")).unwrap(),
        "New $c$ intro.\n\nFirst $a$ paragraph, edited.\n\nSegundo $b$ paragraph.\n"
    );
}

#[test]
fn rejected_import_leaves_the_database_untouched() {
    let (_dir, source, database) = setup(&[("cap/a.tex", "A text.\n")]);
    importer::import_tree(&database, "C", &source, false).unwrap();
    fs::write(source.join("cap/a.tex"), "A changed text.\n").unwrap();
    fs::write(source.join("cap/b.tex"), "B text.\n").unwrap();

    let error = importer::import_tree(&database, "C", &source, false).unwrap_err();

    assert!(error.to_string().contains("cap/a.tex"));
    assert_eq!(file_order(&database), ["cap/a.tex"]);
}

#[test]
fn new_file_is_placed_between_existing_files_without_losing_translations() {
    let (_dir, source, database) = setup(&[("cap/a.tex", "A text.\n"), ("cap/c.tex", "C text.\n")]);
    importer::import_tree(&database, "C", &source, false).unwrap();
    let id = segment_ids(&database, "cap/c.tex")[0];
    translate(&database, id, "C text", "Texto C");
    fs::write(source.join("cap/b.tex"), "B text.\n").unwrap();

    importer::import_tree(&database, "C", &source, false).unwrap();

    assert_eq!(
        file_order(&database),
        ["cap/a.tex", "cap/b.tex", "cap/c.tex"]
    );
    let segment = db::segment(&db::open(&database).unwrap(), id)
        .unwrap()
        .unwrap();
    assert_eq!(segment.translated, "Texto C.\n");
}

#[test]
fn manifest_reorder_keeps_translations() {
    let (dir, source, database) = setup(&[("one/a.tex", "A text.\n"), ("two/b.tex", "B text.\n")]);
    let manifest = dir.path().join("course.tex");
    fs::write(&manifest, "\\activity{one/a.tex}\n\\activity{two/b.tex}\n").unwrap();
    importer::import_tree_with_manifest(&database, "C", &source, false, Some(manifest.as_path()))
        .unwrap();
    let id = segment_ids(&database, "one/a.tex")[0];
    translate(&database, id, "A text", "Texto A");
    fs::write(&manifest, "\\activity{two/b.tex}\n\\activity{one/a.tex}\n").unwrap();

    importer::import_tree_with_manifest(&database, "C", &source, true, Some(manifest.as_path()))
        .unwrap();

    assert_eq!(file_order(&database), ["two/b.tex", "one/a.tex"]);
    let conn = db::open(&database).unwrap();
    let chapters: Vec<String> = db::chapters(&conn)
        .unwrap()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(chapters, ["two", "one"]);
    assert_eq!(
        db::segment(&conn, id).unwrap().unwrap().translated,
        "Texto A.\n"
    );
}
