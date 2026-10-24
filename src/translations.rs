//! Traduções em arquivos JSON versionáveis, aplicadas ao banco sem passar pela
//! tela. Cada entrada aponta para um segmento pelo caminho do arquivo e pela
//! ordem, e carrega o SHA-256 da prosa original para que uma fonte alterada
//! nunca receba uma tradução antiga em silêncio.
use crate::{
    db,
    models::{Placeholder, PlaceholderKind},
    parser,
};
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranslationEntry {
    pub arquivo: String,
    pub ordem: i64,
    /// SHA-256 (hex) de `texto_original`, com os tokens.
    pub hash: String,
    pub traducao: String,
}

/// Tradução de um trecho protegido: ou o texto completo de um token (por
/// exemplo `\dfn{critical point}`), ou o conteúdo de um `\text{...}` dentro
/// de uma fórmula (por exemplo `and`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtectedEntry {
    pub original: String,
    pub traducao: String,
}

pub type ProtectedMap = HashMap<String, String>;

/// Nome do arquivo, dentro do diretório de traduções, com os trechos protegidos.
pub const PROTECTED_FILE: &str = "protegidos.json";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ApplyReport {
    pub entries: usize,
    pub applied: usize,
    /// Segmentos que já tinham texto e foram mantidos (sem `--overwrite`).
    pub kept: usize,
    /// Segmentos cuja tradução no banco já era a do arquivo.
    pub identical: usize,
    pub missing: Vec<String>,
    pub hash_mismatch: Vec<String>,
    pub invalid: Vec<(String, String)>,
}

impl ApplyReport {
    pub fn has_problems(&self) -> bool {
        !self.missing.is_empty() || !self.hash_mismatch.is_empty() || !self.invalid.is_empty()
    }
}

pub fn hash_original(original: &str) -> String {
    format!("{:x}", Sha256::digest(original.as_bytes()))
}

/// Lê um arquivo `.json` ou todos os `.json` de um diretório, em ordem de nome.
pub fn load_entries(path: &Path) -> Result<Vec<TranslationEntry>> {
    let mut files: Vec<PathBuf> = if path.is_dir() {
        WalkDir::new(path)
            .sort_by_file_name()
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_type().is_file()
                    && e.path().extension().is_some_and(|x| x == "json")
                    && e.file_name() != PROTECTED_FILE
            })
            .map(|e| e.into_path())
            .collect()
    } else {
        vec![path.to_path_buf()]
    };
    files.sort();
    let mut entries = Vec::new();
    for file in files {
        let text = fs::read_to_string(&file)
            .with_context(|| format!("lendo traduções em {}", file.display()))?;
        let mut parsed: Vec<TranslationEntry> = serde_json::from_str(&text)
            .with_context(|| format!("JSON inválido em {}", file.display()))?;
        entries.append(&mut parsed);
    }
    if entries.is_empty() {
        bail!("nenhuma tradução encontrada em {}", path.display());
    }
    Ok(entries)
}

/// Lê `protegidos.json` de um diretório de traduções; sem o arquivo, nada.
pub fn load_protected_entries(dir: &Path) -> Result<Vec<ProtectedEntry>> {
    let file = if dir.is_dir() {
        dir.join(PROTECTED_FILE)
    } else {
        return Ok(Vec::new());
    };
    if !file.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&file)
        .with_context(|| format!("lendo trechos protegidos em {}", file.display()))?;
    serde_json::from_str(&text).with_context(|| format!("JSON inválido em {}", file.display()))
}

/// Grava os trechos protegidos no banco; entradas repetidas substituem as anteriores.
pub fn apply_protected_entries(db_path: &Path, entries: &[ProtectedEntry]) -> Result<usize> {
    let mut conn = db::open(db_path)?;
    let tx = conn.transaction()?;
    for entry in entries {
        if entry.original.trim().is_empty() {
            bail!("trecho protegido com original vazio");
        }
        db::upsert_protected_translation(&tx, &entry.original, &entry.traducao)?;
    }
    tx.commit()?;
    Ok(entries.len())
}

const TEXT_IN_MATH: &[&str] = &[
    "\\text{",
    "\\textrm{",
    "\\textbf{",
    "\\textit{",
    "\\textnormal{",
    "\\mbox{",
    "\\mathrm{",
];

/// Devolve o texto de um token protegido com as traduções registradas: o
/// token inteiro, quando houver entrada para ele, ou, em matemática, cada
/// `\text{...}` cujo conteúdo tenha entrada.
pub fn localize_protected(map: &ProtectedMap, placeholder: &Placeholder) -> String {
    if map.is_empty() {
        return placeholder.original.clone();
    }
    if let Some(translated) = map.get(&placeholder.original) {
        return translated.clone();
    }
    match placeholder.kind {
        PlaceholderKind::Math => localize_text_commands(&placeholder.original, map),
        // Figures carry labels (`\\node ... {text}`) and definitions of their
        // own; whole snippets go first so a sentence is matched before the
        // `\\textbf{...}` inside it is localized word by word.
        PlaceholderKind::Environment => {
            localize_text_commands(&localize_snippets(&placeholder.original, map), map)
        }
        _ => placeholder.original.clone(),
    }
}

fn localize_text_commands(source: &str, map: &ProtectedMap) -> String {
    let mut result = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(at) = rest.find('\\') {
        result.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(opener) = TEXT_IN_MATH
            .iter()
            .find(|opener| rest.starts_with(**opener))
        else {
            result.push('\\');
            rest = &rest[1..];
            continue;
        };
        let body_start = opener.len();
        let Some(close) = balanced_close(rest, body_start - 1) else {
            result.push_str(opener);
            rest = &rest[body_start..];
            continue;
        };
        let inner = &rest[body_start..close];
        result.push_str(opener);
        result.push_str(map.get(inner).map_or(inner, String::as_str));
        result.push('}');
        rest = &rest[close + 1..];
    }
    result.push_str(rest);
    result
}

/// Índice do `}` que fecha o `{` em `open`, ignorando chaves escapadas.
fn balanced_close(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Substitui, num trecho de LaTeX bruto (um ambiente protegido ou o espaço
/// entre dois segmentos), cada entrada do mapa cujo original é um comando
/// inteiro (começa com `\\`) ou um argumento inteiro entre chaves (começa com
/// `{`), como o rótulo de um `\\node`. Entradas de texto solto (conteúdo de
/// `\\text{...}`) não se aplicam aqui.
pub fn localize_snippets(source: &str, map: &ProtectedMap) -> String {
    let mut result = source.to_owned();
    let mut keys: Vec<&String> = map
        .keys()
        .filter(|key| {
            (key.starts_with('\\') || key.starts_with('{')) && result.contains(key.as_str())
        })
        .collect();
    // Longer snippets first, so `\\dfn{limit}` never pre-empts `\\dfn{limit at infinity}`.
    keys.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    for key in keys {
        result = result.replace(key.as_str(), &map[key]);
    }
    result
}

/// Os mesmos tokens com os originais localizados, prontos para `reconstruct`.
pub fn localized_placeholders(
    placeholders: &[Placeholder],
    map: &ProtectedMap,
) -> Vec<Placeholder> {
    placeholders
        .iter()
        .map(|placeholder| Placeholder {
            token: placeholder.token.clone(),
            original: localize_protected(map, placeholder),
            kind: placeholder.kind.clone(),
        })
        .collect()
}

/// Aplica as entradas ao banco numa única transação. Sem `overwrite`, só
/// segmentos pendentes e sem texto recebem tradução; o trabalho feito na tela
/// é sempre preservado. Toda tradução passa pela mesma validação de tokens da
/// tela antes de ser gravada.
pub fn apply_entries(
    db_path: &Path,
    entries: &[TranslationEntry],
    overwrite: bool,
) -> Result<ApplyReport> {
    let mut conn = db::open(db_path)?;
    let tx = conn.transaction()?;
    let report = apply_in(&tx, entries, overwrite)?;
    tx.commit()?;
    Ok(report)
}

fn apply_in(
    conn: &Connection,
    entries: &[TranslationEntry],
    overwrite: bool,
) -> Result<ApplyReport> {
    let mut report = ApplyReport {
        entries: entries.len(),
        ..ApplyReport::default()
    };
    let mut file_ids: HashMap<&str, Option<i64>> = HashMap::new();
    for entry in entries {
        let label = format!("{} #{}", entry.arquivo, entry.ordem);
        let file_id = match file_ids.entry(entry.arquivo.as_str()) {
            std::collections::hash_map::Entry::Occupied(o) => *o.get(),
            std::collections::hash_map::Entry::Vacant(v) => {
                *v.insert(db::existing_file(conn, &entry.arquivo)?.map(|f| f.id))
            }
        };
        let Some(file_id) = file_id else {
            report.missing.push(label);
            continue;
        };
        let segment = conn
            .query_row(
                "SELECT id FROM segments WHERE file_id=?1 AND ordem=?2",
                params![file_id, entry.ordem],
                |r| r.get::<_, i64>(0),
            )
            .ok()
            .and_then(|id| db::segment(conn, id).transpose())
            .transpose()?;
        let Some(segment) = segment else {
            report.missing.push(label);
            continue;
        };
        if hash_original(&segment.original) != entry.hash {
            report.hash_mismatch.push(label);
            continue;
        }
        let translation = with_edge_whitespace(&segment.original, &entry.traducao);
        if translation.trim().is_empty() {
            report.invalid.push((label, "tradução vazia".into()));
            continue;
        }
        if let Err(error) = parser::reconstruct(&translation, &segment.placeholders) {
            report.invalid.push((label, error));
            continue;
        }
        if segment.translated == translation && segment.status == "traduzido" {
            report.identical += 1;
            continue;
        }
        if !overwrite && !segment.translated.trim().is_empty() {
            report.kept += 1;
            continue;
        }
        db::save_segment(conn, segment.id, &translation, true)?;
        report.applied += 1;
    }
    Ok(report)
}

/// Os arquivos JSON guardam a tradução sem o espaço das bordas; aqui ele volta
/// do original para que a tela e a exportação vejam o mesmo texto.
fn with_edge_whitespace(original: &str, translation: &str) -> String {
    let leading = &original[..original.len() - original.trim_start().len()];
    let trailing = &original[original.trim_end().len()..];
    format!("{leading}{}{trailing}", translation.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholder(kind: PlaceholderKind, original: &str) -> Placeholder {
        Placeholder {
            token: "{{X_1}}".into(),
            original: original.into(),
            kind,
        }
    }

    #[test]
    fn localizes_whole_commands_and_text_inside_mathematics() {
        let map: ProtectedMap = [
            ("\\dfn{critical point}", "\\dfn{ponto crítico}"),
            ("and", "e"),
            ("if $x<0$,", "se $x<0$,"),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        assert_eq!(
            localize_protected(
                &map,
                &placeholder(PlaceholderKind::Command, "\\dfn{critical point}")
            ),
            "\\dfn{ponto crítico}"
        );
        assert_eq!(
            localize_protected(
                &map,
                &placeholder(
                    PlaceholderKind::Math,
                    "$a \\text{and} b \\text{or} \\textbf{and}$"
                )
            ),
            "$a \\text{e} b \\text{or} \\textbf{e}$"
        );
        assert_eq!(
            localize_protected(
                &map,
                &placeholder(
                    PlaceholderKind::Environment,
                    "\\begin{cases} 1 & \\text{if $x<0$,} \\\\ \\frac{1}{2} \\end{cases}"
                )
            ),
            "\\begin{cases} 1 & \\text{se $x<0$,} \\\\ \\frac{1}{2} \\end{cases}"
        );
        // Prose commands outside mathematics are not searched for `\text`.
        assert_eq!(
            localize_protected(&map, &placeholder(PlaceholderKind::Command, "\\emph{and}")),
            "\\emph{and}"
        );
        assert_eq!(
            localize_protected(
                &map,
                &placeholder(PlaceholderKind::Math, "$\\text{unclosed$")
            ),
            "$\\text{unclosed$"
        );
    }

    #[test]
    fn localizes_whole_snippets_only_in_loose_latex() {
        let map: ProtectedMap = [
            (
                "\\begin{theorem}[Rolle's Theorem]",
                "\\begin{theorem}[Teorema de Rolle]",
            ),
            ("\\dfn{limit}", "\\dfn{limite}"),
            ("\\dfn{limit at infinity}", "\\dfn{limite no infinito}"),
            ("{slope of the}", "{inclinação da}"),
            ("and", "e"),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        assert_eq!(
            localize_snippets(
                "\\begin{theorem}[Rolle's Theorem]\\index{and} \\dfn{limit at infinity} \\node at (1,2) {slope of the};",
                &map
            ),
            "\\begin{theorem}[Teorema de Rolle]\\index{and} \\dfn{limite no infinito} \\node at (1,2) {inclinação da};"
        );
    }

    #[test]
    fn restores_edge_whitespace_from_the_original() {
        assert_eq!(
            with_edge_whitespace("\n  Hello.\n\n", "Olá."),
            "\n  Olá.\n\n"
        );
        assert_eq!(with_edge_whitespace("Hello", "\nOlá\n"), "Olá");
    }
}
