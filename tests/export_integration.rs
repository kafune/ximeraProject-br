use std::fs;
use tempfile::tempdir;
use traduz::{db, export, importer};
#[test]
fn export_without_translations_is_identical() {
    let d = tempdir().unwrap();
    let source = d.path().join("source");
    fs::create_dir(&source).unwrap();
    let input = "\\begin{document}\nHello $x^2$.\n\\end{document}\n";
    fs::write(source.join("one.tex"), input).unwrap();
    fs::write(source.join("image.png"), b"image").unwrap();
    let database = d.path().join("db.sqlite");
    importer::import_tree(&database, "C", &source, false).unwrap();
    let out = d.path().join("out");
    export::export_tree(&database, &source, &out, false).unwrap();
    assert_eq!(fs::read(out.join("one.tex")).unwrap(), input.as_bytes());
    assert_eq!(fs::read(out.join("image.png")).unwrap(), b"image");
    let conn = db::open(&database).unwrap();
    assert!(db::first_pending(&conn).unwrap().is_some());
}

#[test]
fn export_reinjects_protected_content_after_translation() {
    let d = tempdir().unwrap();
    let source = d.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("one.tex"), "Hello \\emph{world} and $x^2$.").unwrap();
    let database = d.path().join("db.sqlite");
    importer::import_tree(&database, "C", &source, false).unwrap();
    let conn = db::open(&database).unwrap();
    let id = db::first_pending(&conn).unwrap().unwrap();
    let segment = db::segment(&conn, id).unwrap().unwrap();
    let translation = segment.original.replace("Hello", "Olá").replace("and", "e");
    db::save_segment(&conn, id, &translation, true).unwrap();
    let out = d.path().join("out");
    export::export_tree(&database, &source, &out, true).unwrap();
    assert_eq!(
        fs::read_to_string(out.join("one.tex")).unwrap(),
        "Olá \\emph{world} e $x^2$."
    );
}
