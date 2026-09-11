use super::Db;
use crate::{
    db,
    models::{FileRecord, ReaderFile, Segment},
    ximera,
};
use askama::Template;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse},
};

#[derive(Debug, Clone)]
struct ReaderActivity {
    id: i64,
    title: String,
    summary: String,
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
    segments: Vec<Segment>,
    translation_segment: Option<i64>,
}

#[derive(Template)]
#[template(path = "reader.html")]
struct ReaderTemplate {
    course: String,
    chapter: String,
    activities: Vec<ReaderActivity>,
    previous: Option<ReaderLink>,
    next: Option<ReaderLink>,
    translation_segment: Option<i64>,
    body_html: String,
}

pub async fn file(State(db_conn): State<Db>, Path(id): Path<i64>) -> impl IntoResponse {
    let result = (|| {
        let reader_data = reader_data(&db_conn, id)?;
        let file = reader_data.file;
        let course_files = reader_data.course_files;
        let segments = reader_data.segments;
        let translation_segment = reader_data.translation_segment;
        let current_index = course_files
            .iter()
            .position(|course_file| course_file.id == id)
            .ok_or_else(|| "arquivo não pertence a um curso".to_string())?;
        let current_file = &course_files[current_index];
        let activities = course_files
            .iter()
            .map(|course_file| ReaderActivity {
                id: course_file.id,
                title: activity_title(course_file),
                summary: activity_summary(course_file),
                current: course_file.id == id,
            })
            .collect::<Vec<_>>();
        let activity_link = |course_file: &ReaderFile| ReaderLink {
            id: course_file.id,
            title: activity_title(course_file),
        };
        let previous = current_index
            .checked_sub(1)
            .map(|index| activity_link(&course_files[index]));
        let next = course_files.get(current_index + 1).map(activity_link);
        let body_html = ximera::render(&file, &segments)?;
        ReaderTemplate {
            course: current_file.course.clone(),
            chapter: display_name(&current_file.chapter),
            activities,
            previous,
            next,
            translation_segment,
            body_html,
        }
        .render()
        .map(Html)
        .map_err(|error| error.to_string())
    })();
    match result {
        Ok(page) => page.into_response(),
        Err(error) if error == "arquivo não encontrado" => {
            (StatusCode::NOT_FOUND, error).into_response()
        }
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}

fn reader_data(db_conn: &Db, id: i64) -> Result<ReaderData, String> {
    let conn = db_conn
        .lock()
        .map_err(|_| "banco indisponível".to_string())?;
    let file = db::file_record(&conn, id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "arquivo não encontrado".to_string())?;
    let course_files = db::reader_course_files(&conn, id).map_err(|error| error.to_string())?;
    let segments = db::segments_for_file(&conn, id).map_err(|error| error.to_string())?;
    let translation_segment =
        db::translation_segment_for_file(&conn, file.id).map_err(|error| error.to_string())?;
    Ok(ReaderData {
        file,
        course_files,
        segments,
        translation_segment,
    })
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

fn activity_title(file: &ReaderFile) -> String {
    file.title_translation
        .as_deref()
        .and_then(|translation| ximera::metadata(translation, &file.title_placeholders).title)
        .unwrap_or_else(|| display_name(&file.path))
}

fn activity_summary(file: &ReaderFile) -> String {
    file.title_translation
        .as_deref()
        .and_then(|translation| ximera::metadata(translation, &file.title_placeholders).summary)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::display_name;

    #[test]
    fn fallback_title_humanizes_a_tex_path() {
        assert_eq!(
            display_name("ximeraTutorial/howIsMyWorkScored.tex"),
            "How Is My Work Scored"
        );
    }
}
