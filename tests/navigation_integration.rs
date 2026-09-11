use std::fs;
use tempfile::tempdir;
use traduz::{db, importer};

#[test]
fn navigation_follows_chapter_order_even_when_ids_do_not() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source");
    for (path, text) in [("one/a.tex", "A1.\n\nA2.\n"), ("two/b.tex", "B1.\n\nB2.\n")] {
        let full = source.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    }
    let database = dir.path().join("db.sqlite3");
    importer::import_tree(&database, "C", &source, false).unwrap();
    let conn = db::open(&database).unwrap();
    // As in production: a chapter imported later (higher id) is placed first.
    conn.execute_batch(
        "UPDATE chapters SET ordem = -1 WHERE nome = 'one';
         UPDATE chapters SET ordem = 0 WHERE nome = 'two';
         UPDATE chapters SET ordem = 1 WHERE nome = 'one';",
    )
    .unwrap();
    let ids = |path: &str| -> Vec<i64> {
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
    };
    let (a, b) = (ids("one/a.tex"), ids("two/b.tex"));

    assert_eq!(db::adjacent_segment(&conn, b[0], true).unwrap(), Some(b[1]));
    assert_eq!(db::adjacent_segment(&conn, b[1], true).unwrap(), Some(a[0]));
    assert_eq!(
        db::adjacent_segment(&conn, a[0], false).unwrap(),
        Some(b[1])
    );
    assert_eq!(db::adjacent_segment(&conn, a[1], true).unwrap(), None);
    assert_eq!(db::adjacent_segment(&conn, b[0], false).unwrap(), None);
    assert_eq!(db::first_pending(&conn).unwrap(), Some(b[0]));
}
