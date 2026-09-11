use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlaceholderKind {
    Math,
    Command,
    Comment,
    Environment,
    Preamble,
    Delimiter,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Placeholder {
    pub token: String,
    pub original: String,
    pub kind: PlaceholderKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Prose(String),
    Protected {
        kind: PlaceholderKind,
        original: String,
    },
}

#[derive(Debug, Clone)]
pub struct ParsedSegment {
    pub original: String,
    pub placeholders: Vec<Placeholder>,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub id: i64,
    pub file_id: i64,
    pub ordem: i64,
    pub original: String,
    pub translated: String,
    pub status: String,
    pub placeholders: Vec<Placeholder>,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone)]
pub struct FileRecord {
    pub id: i64,
    pub path: String,
    pub source: String,
    pub hash: String,
}

/// A file as it appears in the activity strip on a reader page.
#[derive(Debug, Clone)]
pub struct ReaderFile {
    pub id: i64,
    pub path: String,
    pub course: String,
    pub chapter: String,
    /// The first translated segment normally contains the activity title.
    pub title_translation: Option<String>,
    pub title_placeholders: Vec<Placeholder>,
}
