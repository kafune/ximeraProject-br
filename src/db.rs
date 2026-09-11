use crate::models::{FileRecord, ParsedSegment, ReaderFile, Segment};
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::path::Path;

pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut conn =
        Connection::open(path).with_context(|| format!("abrindo banco {}", path.display()))?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    migrate(&mut conn)?;
    Ok(conn)
}
fn migrate(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(include_str!("../migrations/001_inicial.sql"))?;
    Ok(())
}
pub fn course_id(conn: &Connection, name: &str) -> Result<i64> {
    conn.execute(
        "INSERT INTO courses(nome) VALUES (?1) ON CONFLICT(nome) DO NOTHING",
        [name],
    )?;
    Ok(conn.query_row("SELECT id FROM courses WHERE nome=?1", [name], |r| r.get(0))?)
}
pub fn chapter_id(tx: &Transaction<'_>, course: i64, name: &str, order: i64) -> Result<i64> {
    tx.execute("INSERT INTO chapters(course_id,nome,ordem) VALUES (?1,?2,?3) ON CONFLICT(course_id,ordem) DO UPDATE SET nome=excluded.nome", params![course,name,order])?;
    Ok(tx.query_row(
        "SELECT id FROM chapters WHERE course_id=?1 AND ordem=?2",
        params![course, order],
        |r| r.get(0),
    )?)
}
pub fn existing_hash(conn: &Connection, path: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT fonte_hash FROM files WHERE caminho_tex=?1",
            [path],
            |r| r.get(0),
        )
        .optional()?)
}
pub struct NewFile<'a> {
    pub course: i64,
    pub chapter: &'a str,
    pub chapter_order: i64,
    pub path: &'a str,
    pub file_order: i64,
    pub hash: &'a str,
    pub source: &'a str,
    pub segments: &'a [ParsedSegment],
}
pub fn replace_file(conn: &mut Connection, new_file: NewFile<'_>) -> Result<()> {
    let tx = conn.transaction()?;
    let chapter_id = chapter_id(
        &tx,
        new_file.course,
        new_file.chapter,
        new_file.chapter_order,
    )?;
    let old: Option<i64> = tx
        .query_row(
            "SELECT id FROM files WHERE caminho_tex=?1",
            [new_file.path],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = old {
        tx.execute("DELETE FROM files WHERE id=?1", [id])?;
    }
    tx.execute("INSERT INTO files(chapter_id,caminho_tex,ordem,fonte_hash,fonte_original) VALUES(?1,?2,?3,?4,?5)", params![chapter_id,new_file.path,new_file.file_order,new_file.hash,new_file.source])?;
    let file_id = tx.last_insert_rowid();
    for (i, seg) in new_file.segments.iter().enumerate() {
        tx.execute("INSERT INTO segments(file_id,ordem,texto_original,placeholders_json,inicio,fim) VALUES(?1,?2,?3,?4,?5,?6)", params![file_id,i as i64,seg.original,serde_json::to_string(&seg.placeholders)?,seg.start as i64,seg.end as i64])?;
    }
    tx.commit()?;
    Ok(())
}
pub fn file_records(conn: &Connection) -> Result<Vec<FileRecord>> {
    let mut st = conn.prepare(
        "SELECT id,caminho_tex,fonte_original,fonte_hash FROM files ORDER BY caminho_tex",
    )?;
    let records = st
        .query_map([], |r| {
            Ok(FileRecord {
                id: r.get(0)?,
                path: r.get(1)?,
                source: r.get(2)?,
                hash: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(records)
}
pub fn file_record(conn: &Connection, id: i64) -> Result<Option<FileRecord>> {
    Ok(conn
        .query_row(
            "SELECT id,caminho_tex,fonte_original,fonte_hash FROM files WHERE id=?1",
            [id],
            |r| {
                Ok(FileRecord {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    source: r.get(2)?,
                    hash: r.get(3)?,
                })
            },
        )
        .optional()?)
}
/// Returns every activity in the current file's course, in the order used by
/// the importer.  Keeping this query here makes the reader independent from
/// the presentation of the chapter tree used by the translation screen.
pub fn reader_course_files(conn: &Connection, file_id: i64) -> Result<Vec<ReaderFile>> {
    let mut statement = conn.prepare(
        "SELECT f.id, f.caminho_tex, c.nome, ch.nome,
                CASE
                  WHEN s.status = 'traduzido' AND s.texto_traduzido != ''
                  THEN s.texto_traduzido
                END,
                s.placeholders_json
         FROM files f
         JOIN chapters ch ON ch.id = f.chapter_id
         JOIN courses c ON c.id = ch.course_id
         LEFT JOIN segments s ON s.file_id = f.id AND s.ordem = 0
         WHERE c.id = (
           SELECT ch_current.course_id
           FROM files f_current
           JOIN chapters ch_current ON ch_current.id = f_current.chapter_id
           WHERE f_current.id = ?1
         )
         ORDER BY ch.ordem, f.ordem",
    )?;
    let files = statement
        .query_map([file_id], |row| {
            let placeholders = row
                .get::<_, Option<String>>(5)?
                .map(|json| {
                    serde_json::from_str(&json).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })
                })
                .transpose()?
                .unwrap_or_default();
            Ok(ReaderFile {
                id: row.get(0)?,
                path: row.get(1)?,
                course: row.get(2)?,
                chapter: row.get(3)?,
                title_translation: row.get(4)?,
                title_placeholders: placeholders,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(files)
}

/// Chooses a useful place to resume translating an activity: unfinished work
/// first, then its first segment when the activity is already complete.
pub fn translation_segment_for_file(conn: &Connection, file_id: i64) -> Result<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT id FROM segments
             WHERE file_id = ?1
             ORDER BY CASE status
                WHEN 'em_progresso' THEN 0
                WHEN 'pendente' THEN 1
                ELSE 2
             END, ordem
             LIMIT 1",
            [file_id],
            |row| row.get(0),
        )
        .optional()?)
}
pub fn segments_for_file(conn: &Connection, file_id: i64) -> Result<Vec<Segment>> {
    let mut st = conn.prepare("SELECT id,file_id,ordem,texto_original,texto_traduzido,status,placeholders_json,inicio,fim FROM segments WHERE file_id=?1 ORDER BY ordem")?;
    let records = st
        .query_map([file_id], row_segment)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(records)
}
fn row_segment(r: &rusqlite::Row<'_>) -> rusqlite::Result<Segment> {
    let json: String = r.get(6)?;
    let placeholders = serde_json::from_str(&json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(Segment {
        id: r.get(0)?,
        file_id: r.get(1)?,
        ordem: r.get(2)?,
        original: r.get(3)?,
        translated: r.get(4)?,
        status: r.get(5)?,
        placeholders,
        start: r.get::<_, i64>(7)? as usize,
        end: r.get::<_, i64>(8)? as usize,
    })
}
pub fn segment(conn: &Connection, id: i64) -> Result<Option<Segment>> {
    Ok(conn.query_row("SELECT id,file_id,ordem,texto_original,texto_traduzido,status,placeholders_json,inicio,fim FROM segments WHERE id=?1", [id], row_segment).optional()?)
}
pub fn save_segment(conn: &Connection, id: i64, text: &str, complete: bool) -> Result<()> {
    let seg = segment(conn, id)?.context("segmento não encontrado")?;
    if text.is_empty() {
        conn.execute(
            "UPDATE segments SET texto_traduzido='',status='pendente' WHERE id=?1",
            [id],
        )?;
    } else {
        crate::parser::reconstruct(text, &seg.placeholders).map_err(anyhow::Error::msg)?;
        let status = if complete {
            "traduzido"
        } else {
            "em_progresso"
        };
        conn.execute(
            "UPDATE segments SET texto_traduzido=?1,status=?2 WHERE id=?3",
            params![text, status, id],
        )?;
    }
    Ok(())
}
pub fn adjacent_segment(conn: &Connection, id: i64, next: bool) -> Result<Option<i64>> {
    let seg = segment(conn, id)?.context("segmento não encontrado")?;
    let op = if next { ">" } else { "<" };
    let order = if next { "ASC" } else { "DESC" };
    let sql = format!(
        "SELECT id FROM segments WHERE file_id=?1 AND ordem {op} ?2 ORDER BY ordem {order} LIMIT 1"
    );
    let local = conn
        .query_row(&sql, params![seg.file_id, seg.ordem], |r| r.get(0))
        .optional()?;
    if local.is_some() {
        return Ok(local);
    }
    let sql = if next {
        "SELECT s.id FROM segments s JOIN files f ON f.id=s.file_id WHERE (f.chapter_id > (SELECT chapter_id FROM files WHERE id=?1) OR (f.chapter_id=(SELECT chapter_id FROM files WHERE id=?1) AND f.ordem>(SELECT ordem FROM files WHERE id=?1))) ORDER BY f.chapter_id,f.ordem,s.ordem LIMIT 1"
    } else {
        "SELECT s.id FROM segments s JOIN files f ON f.id=s.file_id WHERE (f.chapter_id < (SELECT chapter_id FROM files WHERE id=?1) OR (f.chapter_id=(SELECT chapter_id FROM files WHERE id=?1) AND f.ordem<(SELECT ordem FROM files WHERE id=?1))) ORDER BY f.chapter_id DESC,f.ordem DESC,s.ordem DESC LIMIT 1"
    };
    Ok(conn
        .query_row(sql, [seg.file_id], |r| r.get(0))
        .optional()?)
}
pub fn first_pending(conn: &Connection) -> Result<Option<i64>> {
    Ok(conn.query_row("SELECT id FROM segments ORDER BY CASE status WHEN 'em_progresso' THEN 0 WHEN 'pendente' THEN 1 ELSE 2 END, id LIMIT 1", [], |r| r.get(0)).optional()?)
}
pub type ChapterTree = Vec<(String, Vec<(String, Vec<(i64, i64, String)>)>)>;
pub fn chapters(conn: &Connection) -> Result<ChapterTree> {
    let mut q = conn.prepare("SELECT c.nome,f.caminho_tex,s.id,s.ordem,s.status FROM chapters c JOIN files f ON f.chapter_id=c.id LEFT JOIN segments s ON s.file_id=f.id ORDER BY c.ordem,f.ordem,s.ordem")?;
    let mut answer: ChapterTree = vec![];
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<i64>>(2)?,
            r.get::<_, Option<i64>>(3)?,
            r.get::<_, Option<String>>(4)?,
        ))
    })? {
        let (chapter, file, id, order, status) = row?;
        if answer.last().map(|x| x.0.as_str()) != Some(chapter.as_str()) {
            answer.push((chapter, vec![]));
        }
        let files = &mut answer.last_mut().unwrap().1;
        if files.last().map(|x| x.0.as_str()) != Some(file.as_str()) {
            files.push((file, vec![]));
        }
        if let (Some(id), Some(order), Some(status)) = (id, order, status) {
            files.last_mut().unwrap().1.push((id, order, status));
        }
    }
    Ok(answer)
}
pub fn glossary(conn: &Connection) -> Result<Vec<(String, String)>> {
    let mut q=conn.prepare("SELECT termo_original,termo_traduzido FROM glossary ORDER BY termo_original COLLATE NOCASE")?;
    let terms = q
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(terms)
}
pub fn upsert_glossary(conn: &Connection, original: &str, translated: &str) -> Result<()> {
    conn.execute("INSERT INTO glossary VALUES(?1,?2) ON CONFLICT(termo_original) DO UPDATE SET termo_traduzido=excluded.termo_traduzido",params![original,translated])?;
    Ok(())
}
pub fn delete_glossary(conn: &Connection, original: &str) -> Result<()> {
    conn.execute("DELETE FROM glossary WHERE termo_original=?1", [original])?;
    Ok(())
}
