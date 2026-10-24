use std::fs;
use tempfile::tempdir;
use traduz::{
    db, export, importer,
    translations::{self, TranslationEntry},
};

fn entry(path: &str, ordem: i64, original: &str, traducao: &str) -> TranslationEntry {
    TranslationEntry {
        arquivo: path.into(),
        ordem,
        hash: translations::hash_original(original),
        traducao: traducao.into(),
    }
}

#[test]
fn apply_fills_pending_segments_and_keeps_screen_work() {
    let d = tempdir().unwrap();
    let source = d.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(
        source.join("one.tex"),
        "\\begin{document}\nHello \\emph{world}.\n\nSecond $x$.\n\\end{document}\n",
    )
    .unwrap();
    let database = d.path().join("db.sqlite3");
    importer::import_tree(&database, "C", &source, false).unwrap();
    let conn = db::open(&database).unwrap();
    let file = db::file_records(&conn).unwrap().remove(0);
    let segments = db::segments_for_file(&conn, file.id).unwrap();
    assert_eq!(segments.len(), 2);
    // Work done on the screen must survive.
    let draft = segments[1].original.replace("Second", "Rascunho");
    db::save_segment(&conn, segments[1].id, &draft, false).unwrap();
    drop(conn);

    let first = segments[0].original.trim().replace("Hello", "Olá");
    let second = segments[1].original.trim().replace("Second", "Segundo");
    let entries = vec![
        entry("one.tex", 0, &segments[0].original, &first),
        entry("one.tex", 1, &segments[1].original, &second),
        entry("one.tex", 7, "x", "não existe"),
        entry("one.tex", 0, "fonte antiga", "não aplica"),
    ];
    let report = translations::apply_entries(&database, &entries, false).unwrap();
    assert_eq!(report.applied, 1, "{report:?}");
    assert_eq!(report.kept, 1);
    assert_eq!(report.missing, vec!["one.tex #7".to_string()]);
    assert_eq!(report.hash_mismatch, vec!["one.tex #0".to_string()]);

    let conn = db::open(&database).unwrap();
    let after = db::segments_for_file(&conn, file.id).unwrap();
    assert_eq!(after[0].status, "traduzido");
    assert_eq!(
        after[0].translated,
        segments[0].original.replace("Hello", "Olá")
    );
    assert_eq!(after[1].translated, draft);
    drop(conn);

    let report = translations::apply_entries(&database, &entries[..2], true).unwrap();
    assert_eq!((report.applied, report.identical), (1, 1));
    let out = d.path().join("out");
    export::export_tree(&database, &source, &out, true).unwrap();
    assert_eq!(
        fs::read_to_string(out.join("one.tex")).unwrap(),
        "\\begin{document}\nOlá \\emph{world}.\n\nSegundo $x$.\n\\end{document}\n"
    );
}

#[test]
fn apply_rejects_translations_with_broken_tokens() {
    let d = tempdir().unwrap();
    let source = d.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("one.tex"), "Hello $x$.").unwrap();
    let database = d.path().join("db.sqlite3");
    importer::import_tree(&database, "C", &source, false).unwrap();
    let conn = db::open(&database).unwrap();
    let file = db::file_records(&conn).unwrap().remove(0);
    let original = db::segments_for_file(&conn, file.id).unwrap()[0]
        .original
        .clone();
    drop(conn);
    let entries = vec![
        entry("one.tex", 0, &original, "Olá."),
        entry("one.tex", 0, &original, "   "),
    ];
    let report = translations::apply_entries(&database, &entries, false).unwrap();
    assert_eq!(report.applied, 0);
    assert_eq!(report.invalid.len(), 2);
    assert!(report.has_problems());
}

#[test]
fn load_entries_reads_every_json_in_a_directory() {
    let d = tempdir().unwrap();
    let dir = d.path().join("t");
    fs::create_dir(&dir).unwrap();
    let a = vec![entry("a.tex", 0, "x", "y")];
    let b = vec![entry("b.tex", 0, "x", "y"), entry("b.tex", 1, "z", "w")];
    fs::write(dir.join("b.json"), serde_json::to_string(&b).unwrap()).unwrap();
    fs::write(dir.join("a.json"), serde_json::to_string(&a).unwrap()).unwrap();
    fs::write(dir.join("notas.md"), "ignorado").unwrap();
    let entries = translations::load_entries(&dir).unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].arquivo, "a.tex");
    assert!(translations::load_entries(&d.path().join("vazio.json")).is_err());
}
