use super::Db;
use crate::db;
use askama::Template;
use axum::{
    extract::{Form, Path, State},
    response::{Html, IntoResponse, Redirect},
};
use serde::Deserialize;
#[derive(Template)]
#[template(path = "glossary.html")]
struct GlossaryTemplate {
    terms: Vec<(String, String)>,
}
pub async fn index(State(db_conn): State<Db>) -> impl IntoResponse {
    let c = db_conn.lock().unwrap();
    Html(
        GlossaryTemplate {
            terms: db::glossary(&c).unwrap_or_default(),
        }
        .render()
        .unwrap_or_default(),
    )
}
#[derive(Deserialize)]
pub struct Term {
    original: String,
    translated: String,
}
pub async fn save(State(db_conn): State<Db>, Form(term): Form<Term>) -> impl IntoResponse {
    if !term.original.trim().is_empty() {
        let _ = db::upsert_glossary(&db_conn.lock().unwrap(), &term.original, &term.translated);
    }
    Redirect::to("/glossary")
}
pub async fn delete(State(db_conn): State<Db>, Path(term): Path<String>) -> impl IntoResponse {
    let _ = db::delete_glossary(&db_conn.lock().unwrap(), &term);
    Redirect::to("/glossary")
}
