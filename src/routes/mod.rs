pub mod chapters;
pub mod glossary;
pub mod reader;
pub mod translator;
use axum::{
    routing::{get, post, put},
    Router,
};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};
pub type Db = Arc<Mutex<Connection>>;
pub fn app(db: Db) -> Router {
    Router::new()
        .route("/", get(translator::home))
        .route("/segments/:id", get(translator::show))
        .route("/segments/:id/draft", put(translator::draft))
        .route("/segments/:id/complete", post(translator::complete))
        .route("/segments/:id/skip", post(translator::skip))
        .route("/segments/:id/previous", get(translator::previous))
        .route("/segments/:id/next", get(translator::next))
        .route("/read/files/:id", get(reader::file))
        .route("/chapters", get(chapters::index))
        .route("/glossary", get(glossary::index).post(glossary::save))
        .route("/glossary/:term", post(glossary::delete))
        .nest_service("/static", tower_http::services::ServeDir::new("static"))
        .with_state(db)
}
