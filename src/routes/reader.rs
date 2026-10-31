use super::Db;
use crate::{
    db,
    models::{FileRecord, ReaderFile, Segment, XourseSource},
    translations::ProtectedMap,
    ximera,
    xourse::{self, CardStyle, XourseItem},
};
use askama::Template;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse},
};
use std::collections::HashMap;

/// One card of Ximera's activity strip.
#[derive(Debug, Clone)]
struct ReaderCard {
    id: i64,
    title: String,
    summary: String,
    /// Opens a chapter (`\chapterstyle` in the xourse).
    chapter: bool,
    /// A `\part` heading: no activity behind it.
    part: bool,
    current: bool,
}

#[derive(Debug, Clone)]
struct ReaderLink {
    id: i64,
    title: String,
}

struct ReaderData {
    file: FileRecord,
    course_files: Vec<ReaderFile>,
    xourses: Vec<XourseSource>,
    segments: Vec<Segment>,
    translation_segment: Option<i64>,
    protected: ProtectedMap,
}

/// Navigation around the current activity, derived from the xourse.
struct Outline {
    course: String,
    home: Option<i64>,
    chapter: Option<ReaderLink>,
    title: String,
    cards: Vec<ReaderCard>,
    previous: Option<ReaderLink>,
    next: Option<ReaderLink>,
}

#[derive(Template)]
#[template(path = "reader.html")]
struct ReaderTemplate {
    course: String,
    home: Option<i64>,
    chapter: Option<ReaderLink>,
    title: String,
    path: String,
    cards: Vec<ReaderCard>,
    previous: Option<ReaderLink>,
    next: Option<ReaderLink>,
    translation_segment: Option<i64>,
    body_html: String,
    stale: bool,
}

pub async fn file(State(db_conn): State<Db>, Path(id): Path<i64>) -> impl IntoResponse {
    match page(&db_conn, id) {
        Ok(page) => Html(page).into_response(),
        Err(error) if error == "arquivo não encontrado" => {
            (StatusCode::NOT_FOUND, error).into_response()
        }
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}

fn page(db_conn: &Db, id: i64) -> Result<String, String> {
    let data = reader_data(db_conn, id)?;
    // Compiling can take a while; the database lock was released above.
    let rendered = ximera::render(&data.file, &data.segments, &data.protected)?;
    let outline = Outline::build(&data.course_files, &data.xourses, id);
    ReaderTemplate {
        course: outline.course,
        home: outline.home,
        chapter: outline.chapter,
        title: outline.title,
        path: data.file.path,
        cards: outline.cards,
        previous: outline.previous,
        next: outline.next,
        translation_segment: data.translation_segment,
        body_html: rendered.html,
        stale: rendered.stale,
    }
    .render()
    .map_err(|error| error.to_string())
}

fn reader_data(db_conn: &Db, id: i64) -> Result<ReaderData, String> {
    let conn = db_conn
        .lock()
        .map_err(|_| "banco indisponível".to_string())?;
    let file = db::file_record(&conn, id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "arquivo não encontrado".to_string())?;
    let course_files = db::reader_course_files(&conn, id).map_err(|error| error.to_string())?;
    let xourses = db::xourse_sources(&conn, id).map_err(|error| error.to_string())?;
    let segments = db::segments_for_file(&conn, id).map_err(|error| error.to_string())?;
    let translation_segment =
        db::translation_segment_for_file(&conn, file.id).map_err(|error| error.to_string())?;
    let protected = db::protected_translations(&conn).map_err(|error| error.to_string())?;
    Ok(ReaderData {
        file,
        course_files,
        xourses,
        segments,
        translation_segment,
        protected,
    })
}

impl Outline {
    fn build(files: &[ReaderFile], xourses: &[XourseSource], current_id: i64) -> Outline {
        let current = files.iter().find(|file| file.id == current_id);
        let by_path: HashMap<&str, &ReaderFile> = files
            .iter()
            .map(|file| (file.path.as_str(), file))
            .collect();
        let parsed: Vec<(&XourseSource, Vec<XourseItem>)> = xourses
            .iter()
            .map(|source| (source, xourse::parse(&source.source)))
            .collect();
        let listed = |items: &[XourseItem]| {
            xourse::activity_paths(items)
                .filter(|path| by_path.contains_key(path))
                .count()
        };
        // Prefer the xourse that lists the current activity; the course may
        // also contain exercise lists written as small xourses.
        let chosen = parsed
            .iter()
            .filter(|(_, items)| {
                current
                    .is_some_and(|file| xourse::activity_paths(items).any(|path| path == file.path))
            })
            .max_by_key(|(_, items)| listed(items))
            .or_else(|| parsed.iter().max_by_key(|(_, items)| listed(items)))
            .filter(|(_, items)| listed(items) > 0);
        let course_name = current
            .or(files.first())
            .map(|file| file.course.clone())
            .unwrap_or_default();
        let (course, items) = match chosen {
            Some((source, items)) => (
                files
                    .iter()
                    .find(|file| file.id == source.id)
                    .and_then(translated_title)
                    .unwrap_or(course_name),
                items.clone(),
            ),
            None => (
                course_name,
                files
                    .iter()
                    .filter(|file| file.is_activity)
                    .map(|file| XourseItem::Activity {
                        path: file.path.clone(),
                        style: if file.path.ends_with("titlePage.tex") {
                            CardStyle::Chapter
                        } else {
                            CardStyle::Section
                        },
                    })
                    .collect(),
            ),
        };

        let mut cards = Vec::new();
        let mut chapter = None;
        let mut current_index = None;
        for item in &items {
            match item {
                XourseItem::Part { title } => cards.push(ReaderCard {
                    id: 0,
                    title: title.clone(),
                    summary: String::new(),
                    chapter: false,
                    part: true,
                    current: false,
                }),
                XourseItem::Activity { path, style } => {
                    let Some(file) = by_path.get(path.as_str()) else {
                        continue;
                    };
                    let is_current = file.id == current_id;
                    let title = activity_title(file);
                    if *style == CardStyle::Chapter && current_index.is_none() {
                        chapter = (!is_current).then(|| ReaderLink {
                            id: file.id,
                            title: title.clone(),
                        });
                    }
                    if is_current {
                        current_index = Some(cards.len());
                    }
                    cards.push(ReaderCard {
                        id: file.id,
                        title,
                        summary: activity_summary(file),
                        chapter: *style == CardStyle::Chapter,
                        part: false,
                        current: is_current,
                    });
                }
            }
        }
        let link = |card: &ReaderCard| ReaderLink {
            id: card.id,
            title: card.title.clone(),
        };
        let previous = current_index.and_then(|index| {
            cards[..index]
                .iter()
                .rev()
                .find(|card| !card.part)
                .map(link)
        });
        let next = current_index
            .and_then(|index| cards[index + 1..].iter().find(|card| !card.part).map(link));
        let title = current.map(activity_title).unwrap_or_default();
        Outline {
            course,
            home: cards.iter().find(|card| !card.part).map(|card| card.id),
            chapter,
            title,
            cards,
            previous,
            next,
        }
    }
}

fn display_name(path: &str) -> String {
    let stem = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".tex")
        .replace('_', " ");
    let mut result = String::new();
    for (index, character) in stem.chars().enumerate() {
        if index > 0 && character.is_ascii_uppercase() {
            result.push(' ');
        }
        result.push(character);
    }
    let mut characters = result.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => result,
    }
}

fn translated_title(file: &ReaderFile) -> Option<String> {
    file.title_translation
        .as_deref()
        .and_then(|translation| ximera::metadata(translation, &file.title_placeholders).title)
}

fn activity_title(file: &ReaderFile) -> String {
    translated_title(file).unwrap_or_else(|| display_name(&file.path))
}

fn activity_summary(file: &ReaderFile) -> String {
    file.title_translation
        .as_deref()
        .and_then(|translation| ximera::metadata(translation, &file.title_placeholders).summary)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_title_humanizes_a_tex_path() {
        assert_eq!(
            display_name("ximeraTutorial/howIsMyWorkScored.tex"),
            "How Is My Work Scored"
        );
    }

    fn reader_file(id: i64, path: &str, title: &str, is_activity: bool) -> ReaderFile {
        ReaderFile {
            id,
            path: path.into(),
            course: "Calculus 1".into(),
            chapter: path.split('/').next().unwrap_or("").into(),
            title_translation: Some(format!("{title}{{{{CMD_1}}}}")),
            title_placeholders: vec![crate::models::Placeholder {
                token: "{{CMD_1}}".into(),
                original: "}".into(),
                kind: crate::models::PlaceholderKind::Command,
            }],
            is_activity,
        }
    }

    #[test]
    fn outline_follows_the_xourse_and_skips_files_outside_it() {
        let files = vec![
            reader_file(1, "calculus1.tex", "Cálculo 1", false),
            reader_file(2, "limits/titlePage.tex", "Limites", true),
            reader_file(3, "limits/breakGround.tex", "Preparar o terreno", true),
            reader_file(4, "limits/digIn.tex", "Aprofundar", true),
            reader_file(5, "limits/exercises/ex1.tex", "Exercício solto", true),
        ];
        let xourses = vec![XourseSource {
            id: 1,
            path: "calculus1.tex".into(),
            source: "\\documentclass{xourse}\\title{Calculus 1}\\begin{document}\n\
                     \\part{Parte 1}\n\\chapterstyle\n\\activity{limits/titlePage.tex}\n\
                     \\sectionstyle\n\\activity{limits/breakGround.tex}\n\\activity{limits/digIn.tex}\n\
                     \\activity{limits/missing.tex}\n\\end{document}"
                .into(),
        }];
        let outline = Outline::build(&files, &xourses, 3);
        assert_eq!(outline.course, "Cálculo 1");
        assert_eq!(outline.home, Some(2));
        assert_eq!(outline.title, "Preparar o terreno");
        assert_eq!(
            outline
                .chapter
                .as_ref()
                .map(|link| (link.id, link.title.as_str())),
            Some((2, "Limites"))
        );
        let titles: Vec<(&str, bool, bool, bool)> = outline
            .cards
            .iter()
            .map(|card| (card.title.as_str(), card.part, card.chapter, card.current))
            .collect();
        assert_eq!(
            titles,
            vec![
                ("Parte 1", true, false, false),
                ("Limites", false, true, false),
                ("Preparar o terreno", false, false, true),
                ("Aprofundar", false, false, false),
            ]
        );
        assert_eq!(outline.previous.as_ref().map(|link| link.id), Some(2));
        assert_eq!(outline.next.as_ref().map(|link| link.id), Some(4));

        // The chapter page itself is not its own breadcrumb.
        let chapter_page = Outline::build(&files, &xourses, 2);
        assert!(chapter_page.chapter.is_none());
        assert!(chapter_page.previous.is_none());
    }

    #[test]
    fn outline_without_a_xourse_lists_every_activity() {
        let files = vec![
            reader_file(1, "a/titlePage.tex", "A", true),
            reader_file(2, "a/digIn.tex", "Aprofundar", true),
            reader_file(3, "preamble.tex", "", false),
        ];
        let outline = Outline::build(&files, &[], 2);
        assert_eq!(outline.course, "Calculus 1");
        assert_eq!(outline.cards.len(), 2);
        assert!(outline.cards[0].chapter);
        assert_eq!(outline.previous.as_ref().map(|link| link.id), Some(1));
        assert!(outline.next.is_none());
    }
}
