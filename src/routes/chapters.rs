use super::Db;
use crate::db::{self, ChapterTree};
use askama::Template;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
};
#[derive(Template)]
#[template(path = "chapters.html")]
struct ChaptersTemplate {
    chapters: ChapterTree,
}
pub async fn index(State(db_conn): State<Db>) -> impl IntoResponse {
    let result = (|| {
        let c = db_conn
            .lock()
            .map_err(|_| "banco indisponível".to_string())?;
        let chapters = db::chapters(&c).map_err(|e| e.to_string())?;
        ChaptersTemplate { chapters }
            .render()
            .map(Html)
            .map_err(|e| e.to_string())
    })();
    match result {
        Ok(x) => x.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}
