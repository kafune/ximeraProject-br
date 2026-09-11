use super::Db;
use crate::{
    db::{self, ChapterTree},
    models::Segment,
};
use askama::Template;
use axum::{
    extract::{Form, Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use serde::Deserialize;

#[derive(Template)]
#[template(path = "translator.html")]
struct TranslatorTemplate {
    segment: Segment,
    progress: String,
    glossary: Vec<(String, String)>,
    chapters: ChapterTree,
}
fn render(db_conn: &Db, id: i64) -> Result<Html<String>, (StatusCode, String)> {
    let conn = db_conn.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "banco indisponível".into(),
        )
    })?;
    let s = db::segment(&conn, id)
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "segmento não encontrado".into()))?;
    let pending: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM segments WHERE status != 'traduzido'",
            [],
            |r| r.get(0),
        )
        .map_err(internal)?;
    let all: i64 = conn
        .query_row("SELECT COUNT(*) FROM segments", [], |r| r.get(0))
        .map_err(internal)?;
    let glossary = db::glossary(&conn).map_err(internal)?;
    let chapters = db::chapters(&conn).map_err(internal)?;
    TranslatorTemplate {
        segment: s,
        progress: format!("{} pendentes de {}", pending, all),
        glossary,
        chapters,
    }
    .render()
    .map(Html)
    .map_err(internal)
}
fn internal(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
pub async fn home(State(db_conn): State<Db>) -> Response {
    let id = {
        let c = match db_conn.lock() {
            Ok(c) => c,
            Err(_) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, "banco indisponível").into_response()
            }
        };
        match db::first_pending(&c) {
            Ok(Some(x)) => x,
            Ok(None) => {
                return Html("<p>Nenhum segmento importado.</p>".to_string()).into_response()
            }
            Err(e) => return internal(e).into_response(),
        }
    };
    Redirect::to(&format!("/segments/{id}")).into_response()
}
pub async fn show(State(db_conn): State<Db>, Path(id): Path<i64>) -> Response {
    match render(&db_conn, id) {
        Ok(x) => x.into_response(),
        Err(e) => e.into_response(),
    }
}
#[derive(Deserialize)]
pub struct Translation {
    text: String,
}
async fn save(db_conn: Db, id: i64, form: Translation, complete: bool) -> Response {
    let r = {
        let c = match db_conn.lock() {
            Ok(c) => c,
            Err(_) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, "banco indisponível").into_response()
            }
        };
        db::save_segment(&c, id, &form.text, complete)
    };
    match r {
        Ok(()) => {
            if complete {
                let next = {
                    let c = db_conn.lock().unwrap();
                    db::adjacent_segment(&c, id, true).ok().flatten()
                };
                if let Some(x) = next {
                    return Redirect::to(&format!("/segments/{x}")).into_response();
                }
            }
            (StatusCode::NO_CONTENT, "").into_response()
        }
        Err(e) => (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()).into_response(),
    }
}
pub async fn draft(
    State(db_conn): State<Db>,
    Path(id): Path<i64>,
    Form(form): Form<Translation>,
) -> Response {
    save(db_conn, id, form, false).await
}
pub async fn complete(
    State(db_conn): State<Db>,
    Path(id): Path<i64>,
    Form(form): Form<Translation>,
) -> Response {
    save(db_conn, id, form, true).await
}
pub async fn skip(State(db_conn): State<Db>, Path(id): Path<i64>) -> Response {
    let n = {
        let c = db_conn.lock().unwrap();
        db::adjacent_segment(&c, id, true).ok().flatten()
    };
    n.map(|x| Redirect::to(&format!("/segments/{x}")).into_response())
        .unwrap_or_else(|| Redirect::to("/").into_response())
}
pub async fn previous(State(db_conn): State<Db>, Path(id): Path<i64>) -> Response {
    let n = {
        let c = db_conn.lock().unwrap();
        db::adjacent_segment(&c, id, false).ok().flatten()
    };
    n.map(|x| Redirect::to(&format!("/segments/{x}")).into_response())
        .unwrap_or_else(|| Redirect::to("/").into_response())
}
pub async fn next(State(db_conn): State<Db>, Path(id): Path<i64>) -> Response {
    skip(State(db_conn), Path(id)).await
}
