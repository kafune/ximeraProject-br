use std::fs;
use tempfile::tempdir;
use traduz::{db, importer, xourse};

#[test]
fn reader_data_distinguishes_the_xourse_from_activities_and_fragments() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source");
    let files = [
        (
            "calculus1.tex",
            "\\documentclass{xourse}\n\\title{Calculus 1}\n\\begin{document}\n\\chapterstyle\n\\activity{limits/titlePage.tex}\n\\sectionstyle\n\\activity{limits/digIn.tex}\n\\end{document}\n",
        ),
        (
            "limits/titlePage.tex",
            "\\documentclass{ximera}\n\\title{Limits}\n\\begin{document}\n\\maketitle\nIntro.\n\\end{document}\n",
        ),
        (
            "limits/digIn.tex",
            "\\documentclass{ximera}\n\\title{Dig in}\n\\begin{document}\n\\begin{abstract}\nWe dig in.\n\\end{abstract}\n\\maketitle\nText.\n\\end{document}\n",
        ),
        ("limits/exercises/list.tex", "Loose prose only.\n"),
    ];
    for (path, text) in files {
        let full = source.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    }
    let database = dir.path().join("db.sqlite3");
    importer::import_tree_with_manifest(
        &database,
        "C",
        &source,
        false,
        Some(&source.join("calculus1.tex")),
    )
    .unwrap();
    let conn = db::open(&database).unwrap();
    let dig_in = db::file_records(&conn)
        .unwrap()
        .into_iter()
        .find(|file| file.path == "limits/digIn.tex")
        .unwrap();

    let xourses = db::xourse_sources(&conn, dig_in.id).unwrap();
    assert_eq!(xourses.len(), 1);
    assert_eq!(xourses[0].path, "calculus1.tex");
    assert_eq!(
        xourse::activity_paths(&xourse::parse(&xourses[0].source)).collect::<Vec<_>>(),
        ["limits/titlePage.tex", "limits/digIn.tex"]
    );

    let course_files = db::reader_course_files(&conn, dig_in.id).unwrap();
    let flags: Vec<(&str, bool)> = course_files
        .iter()
        .map(|file| (file.path.as_str(), file.is_activity))
        .collect();
    assert_eq!(
        flags,
        [
            ("limits/titlePage.tex", true),
            ("limits/digIn.tex", true),
            ("limits/exercises/list.tex", false),
            ("calculus1.tex", false),
        ]
    );
    let chapters = db::chapters(&conn).unwrap();
    assert_eq!(chapters[0].1[0].1, course_files[0].id);
}
