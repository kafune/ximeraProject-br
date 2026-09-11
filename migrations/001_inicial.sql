PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS courses (
  id INTEGER PRIMARY KEY,
  nome TEXT NOT NULL UNIQUE
);
CREATE TABLE IF NOT EXISTS chapters (
  id INTEGER PRIMARY KEY,
  course_id INTEGER NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
  nome TEXT NOT NULL,
  ordem INTEGER NOT NULL,
  UNIQUE(course_id, ordem)
);
CREATE INDEX IF NOT EXISTS idx_chapters_course_ordem ON chapters(course_id, ordem);
CREATE TABLE IF NOT EXISTS files (
  id INTEGER PRIMARY KEY,
  chapter_id INTEGER NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
  caminho_tex TEXT NOT NULL UNIQUE,
  ordem INTEGER NOT NULL,
  fonte_hash TEXT NOT NULL,
  fonte_original TEXT NOT NULL DEFAULT '',
  UNIQUE(chapter_id, ordem)
);
CREATE INDEX IF NOT EXISTS idx_files_chapter_ordem ON files(chapter_id, ordem);
CREATE TABLE IF NOT EXISTS segments (
  id INTEGER PRIMARY KEY,
  file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  ordem INTEGER NOT NULL,
  texto_original TEXT NOT NULL,
  texto_traduzido TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'pendente'
    CHECK(status IN ('pendente', 'em_progresso', 'traduzido')),
  placeholders_json TEXT NOT NULL DEFAULT '[]',
  inicio INTEGER NOT NULL DEFAULT 0,
  fim INTEGER NOT NULL DEFAULT 0,
  UNIQUE(file_id, ordem)
);
CREATE INDEX IF NOT EXISTS idx_segments_file_ordem ON segments(file_id, ordem);
CREATE INDEX IF NOT EXISTS idx_segments_status ON segments(status);
CREATE TABLE IF NOT EXISTS glossary (
  termo_original TEXT PRIMARY KEY COLLATE NOCASE,
  termo_traduzido TEXT NOT NULL
);
