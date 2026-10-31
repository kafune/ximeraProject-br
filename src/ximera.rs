//! Rendering bridge for Ximera activities.
//!
//! Ximera's presentation is produced by its `ximeraLatex` TeX4ht rules, not by
//! a hand-written LaTeX renderer.  This module compiles an activity with that
//! same toolchain, running the two steps of `xmlatex texHtml` (`pdflatex` for
//! the TikZ figures, then `make4ht` with the `ximera` configuration), either
//! with a local TeX installation or inside the official Docker image.  The
//! resulting fragment and its images are stored under `rendered/`, so the web
//! server never needs TeX at request time.  Only completed Traduz segments are
//! injected; drafts and pending prose keep the original text, exactly as in
//! the export.

use crate::{
    export,
    models::{FileRecord, Placeholder, Segment},
    parser,
    translations::ProtectedMap,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Component, Path, PathBuf},
    process::Command,
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};
use walkdir::WalkDir;

/// Course trees shipped with the repository, in order of preference.
const SOURCE_ROOTS: [&str; 2] = [
    "courses/ximera-calculus-1-calculus1",
    "courses/ximera-calculus-1",
];
const DEFAULT_RENDERED_ROOT: &str = "rendered";
/// URL prefix under which the rendered tree is served.
pub const RENDERED_URL: &str = "/rendered";
const DEFAULT_IMAGE: &str = "ghcr.io/ximeraproject/ximeralatex:v2.7.2";
const SIGNATURE_PREFIX: &str = "<!-- traduz-render:";
/// Bump when the post-processing below changes, so cached pages are rebuilt.
const RENDER_VERSION: &[u8] = b"traduz-ximera-render-v4";
const COMPILE_TIMEOUT: &str = "900s";
const ANSWER_ID_PREFIX: &str = "xm-answer-";

#[derive(Debug, Clone, Default)]
pub struct ActivityMetadata {
    pub title: Option<String>,
    pub summary: Option<String>,
}

pub fn metadata(translation: &str, placeholders: &[Placeholder]) -> ActivityMetadata {
    let source = parser::reconstruct(translation, placeholders).ok();
    ActivityMetadata {
        // The segmenter starts an editable range *inside* `\\title{...}`, so
        // the opening command is deliberately not in the stored segment.  The
        // first prose piece is therefore the title; the compiler still sees
        // the original opening command when rendering the full source.
        title: source
            .as_deref()
            .and_then(|value| command_argument(value, "title"))
            .and_then(plain_text)
            .or_else(|| leading_prose(translation).and_then(plain_text)),
        summary: source
            .as_deref()
            .and_then(|value| environment_body(value, "abstract"))
            .and_then(plain_text)
            .or_else(|| abstract_prose(translation, placeholders).and_then(plain_text)),
    }
}

/// A standalone Ximera activity, as opposed to a xourse, a preamble or an
/// exercise list that is only `\input` by other files.
pub fn is_activity(source: &str) -> bool {
    source.contains("\\documentclass") && source.contains("{ximera}")
}

#[derive(Debug, Clone)]
pub struct Rendered {
    pub html: String,
    /// The translation changed after this page was compiled and no compiler
    /// was available, so the previous rendering is shown.
    pub stale: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The stored page already matched the current translation.
    Cached,
    Compiled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Renderer {
    /// `pdflatex` and `make4ht` with `ximera.cls` on this machine.
    Local,
    /// The official `ximeralatex` image.
    Docker,
    Unavailable,
}

impl Renderer {
    /// Honors `TRADUZ_XIMERA_RENDERER` (`local`, `docker` or `none`); otherwise
    /// prefers a local toolchain and falls back to Docker.
    pub fn detect() -> Renderer {
        static DETECTED: OnceLock<Renderer> = OnceLock::new();
        *DETECTED.get_or_init(|| match env::var("TRADUZ_XIMERA_RENDERER").as_deref() {
            Ok("local") => Renderer::Local,
            Ok("docker") => Renderer::Docker,
            Ok("none") => Renderer::Unavailable,
            _ if local_toolchain_available() => Renderer::Local,
            _ if command_exists("docker") => Renderer::Docker,
            _ => Renderer::Unavailable,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Renderer::Local => "local (pdflatex + make4ht)",
            Renderer::Docker => "docker (ximeralatex)",
            Renderer::Unavailable => "indisponível",
        }
    }
}

fn local_toolchain_available() -> bool {
    command_exists("pdflatex")
        && command_exists("make4ht")
        && Command::new("kpsewhich")
            .arg("ximera.cls")
            .output()
            .is_ok_and(|output| output.status.success() && !output.stdout.is_empty())
}

fn command_exists(program: &str) -> bool {
    Command::new(program).arg("--version").output().is_ok()
}

pub fn rendered_root() -> PathBuf {
    env::var_os("TRADUZ_XIMERA_RENDERED")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_RENDERED_ROOT))
}

pub fn source_root() -> Result<PathBuf, String> {
    if let Some(configured) = env::var_os("TRADUZ_XIMERA_SOURCE") {
        return Ok(PathBuf::from(configured));
    }
    SOURCE_ROOTS
        .iter()
        .map(PathBuf::from)
        .find(|root| root.is_dir())
        .ok_or_else(|| {
            "fonte do curso Ximera não encontrada; defina TRADUZ_XIMERA_SOURCE".to_string()
        })
}

/// Renders for the reader: the stored page when it is current, otherwise a
/// fresh compilation, and the previous page when compiling is impossible.
pub fn render(
    file: &FileRecord,
    segments: &[Segment],
    protected: &ProtectedMap,
) -> Result<Rendered, String> {
    let prepared = Prepared::new(file, segments, protected)?;
    if let Some(html) = prepared.cached_current() {
        return Ok(Rendered { html, stale: false });
    }
    match prepared.compile(Renderer::detect()) {
        Ok(rendered) => Ok(rendered),
        Err(error) => match prepared.cached {
            Some(html) => {
                eprintln!(
                    "ximera: mostrando a versão anterior de {}: {error}",
                    file.path
                );
                Ok(Rendered { html, stale: true })
            }
            None => Err(error),
        },
    }
}

/// Renders for the batch command: never falls back to a previous page.
pub fn render_file(
    file: &FileRecord,
    segments: &[Segment],
    protected: &ProtectedMap,
    renderer: Renderer,
    force: bool,
) -> Result<Outcome, String> {
    let prepared = Prepared::new(file, segments, protected)?;
    if !force && prepared.cached_current().is_some() {
        return Ok(Outcome::Cached);
    }
    prepared.compile(renderer)?;
    Ok(Outcome::Compiled)
}

struct Prepared {
    relative: PathBuf,
    source: String,
    signature: String,
    target: PathBuf,
    cached: Option<String>,
}

impl Prepared {
    fn new(
        file: &FileRecord,
        segments: &[Segment],
        protected: &ProtectedMap,
    ) -> Result<Prepared, String> {
        ensure_safe_translations(segments)?;
        let source = export::translated_source(&file.source, segments, protected)?;
        let relative = safe_relative_path(&file.path)?;
        let signature = signature(&file.path, &source);
        let target = rendered_root().join(relative.with_extension("html"));
        let cached = fs::read_to_string(&target).ok();
        Ok(Prepared {
            relative,
            source,
            signature,
            target,
            cached,
        })
    }

    fn cached_current(&self) -> Option<String> {
        self.cached
            .as_ref()
            .filter(|html| cached_signature(html) == Some(self.signature.as_str()))
            .cloned()
    }

    fn compile(&self, renderer: Renderer) -> Result<Rendered, String> {
        let toolchain = match renderer {
            Renderer::Local => Toolchain::Local,
            Renderer::Docker => Toolchain::Docker,
            Renderer::Unavailable => {
                return Err(
                    "nenhum compilador do Ximera disponível: instale pdflatex e make4ht \
                            com o ximeraLatex, ou o Docker"
                        .to_string(),
                )
            }
        };
        let compiled = compile(&self.relative, &self.source, toolchain)?;
        let url_directory = match self.relative.parent().filter(|p| !p.as_os_str().is_empty()) {
            Some(directory) => format!("{RENDERED_URL}/{}", directory.to_string_lossy()),
            None => RENDERED_URL.to_owned(),
        };
        let html = finish(&compiled, &url_directory, &self.signature)?;
        let directory = self
            .target
            .parent()
            .ok_or_else(|| "destino da renderização inválido".to_string())?;
        fs::create_dir_all(directory).map_err(io_error("criando a pasta de renderização"))?;
        for image in &compiled.images {
            fs::write(directory.join(&image.name), &image.bytes)
                .map_err(io_error("gravando imagem renderizada"))?;
        }
        fs::write(&self.target, &html).map_err(io_error("gravando a atividade renderizada"))?;
        Ok(Rendered { html, stale: false })
    }
}

/// User-entered backslashes would be executable TeX when the official compiler
/// runs. Commands required by the document already live in protected tokens,
/// so reject new ones before entering the compiler.
fn ensure_safe_translations(segments: &[Segment]) -> Result<(), String> {
    for segment in segments {
        if segment.is_complete() && segment.translated.contains('\\') {
            return Err(format!(
                "segmento {} contém um comando TeX fora dos tokens protegidos",
                segment.id
            ));
        }
    }
    Ok(())
}

fn signature(path: &str, source: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(RENDER_VERSION);
    hasher.update([0]);
    hasher.update(path.as_bytes());
    hasher.update([0]);
    hasher.update(source.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn cached_signature(html: &str) -> Option<&str> {
    let rest = html.strip_prefix(SIGNATURE_PREFIX)?;
    let end = rest.find("-->")?;
    Some(rest[..end].trim())
}

fn safe_relative_path(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
    {
        return Err("caminho de atividade fora da fonte do curso".to_string());
    }
    Ok(path.to_owned())
}

#[derive(Debug, Clone, Copy)]
enum Toolchain {
    Local,
    Docker,
}

struct Compiled {
    html: String,
    /// `\newcommand`s collected by ximeraLatex for MathJax (`.xmjax`).
    macros: String,
    images: Vec<Image>,
}

struct Image {
    /// The `src` value as written by TeX4ht.
    reference: String,
    name: String,
    bytes: Vec<u8>,
}

fn compile(relative: &Path, source: &str, toolchain: Toolchain) -> Result<Compiled, String> {
    let source_root = source_root()?
        .canonicalize()
        .map_err(io_error("abrindo a fonte do curso Ximera"))?;
    let work = env::temp_dir().join(format!(
        "traduz-ximera-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "relógio do sistema inválido".to_string())?
            .as_nanos()
    ));
    let result = compile_in(&work, &source_root, relative, source, toolchain);
    let _ = fs::remove_dir_all(&work);
    result
}

fn compile_in(
    work: &Path,
    source_root: &Path,
    relative: &Path,
    source: &str,
    toolchain: Toolchain,
) -> Result<Compiled, String> {
    // Activities `\input` the course preamble and shared exercises, so the
    // whole tree is copied, as the Docker build mounts the whole repository.
    copy_tree(source_root, work)?;
    let activity = work.join(relative);
    let directory = activity
        .parent()
        .ok_or_else(|| "caminho de atividade inválido".to_string())?;
    fs::create_dir_all(directory).map_err(io_error("criando a pasta temporária da atividade"))?;
    fs::write(&activity, source).map_err(io_error("gravando atividade traduzida temporária"))?;
    let filename = relative
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "nome de atividade inválido".to_string())?;
    match toolchain {
        Toolchain::Local => run_local(directory, filename)?,
        Toolchain::Docker => run_docker(work, relative, filename)?,
    }
    let html = fs::read_to_string(activity.with_extension("html"))
        .map_err(io_error("lendo o HTML gerado pelo Ximera"))?;
    let macros = fs::read_to_string(activity.with_extension("xmjax")).unwrap_or_default();
    let images = collect_images(&html, directory, work)?;
    Ok(Compiled {
        html,
        macros,
        images,
    })
}

fn copy_tree(source_root: &Path, work: &Path) -> Result<(), String> {
    for entry in WalkDir::new(source_root)
        .into_iter()
        .filter_entry(|entry| entry.file_name() != ".git")
    {
        let entry = entry.map_err(|error| format!("lendo a fonte do curso: {error}"))?;
        let relative = entry
            .path()
            .strip_prefix(source_root)
            .map_err(|error| error.to_string())?;
        let destination = work.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination)
                .map_err(io_error("criando cópia temporária do curso"))?;
        } else if entry.file_type().is_file() {
            fs::copy(entry.path(), &destination).map_err(io_error("copiando a fonte do curso"))?;
        }
    }
    // `xmlatex` (the official Ximera Docker wrapper) refuses to run unless
    // `git rev-parse --show-toplevel` succeeds inside /code, to confirm it
    // was started from a repo root. Copying the real `.git` (tens of
    // megabytes) on every render would be wasteful and `xmlatex` only checks
    // that it exists, not its contents, so an empty repo is initialized
    // here instead.
    let output = Command::new("git")
        .arg("init")
        .arg("-q")
        .arg(work)
        .output()
        .map_err(io_error("inicializando o git da cópia temporária"))?;
    if !output.status.success() {
        return Err(command_error("git init", &output.stdout, &output.stderr));
    }
    Ok(())
}

/// The two steps of `xmlatex texHtml`: one `pdflatex` pass exports the TikZ
/// figures, then `make4ht` with the `ximera` configuration writes the HTML.
fn run_local(directory: &Path, filename: &str) -> Result<(), String> {
    let input = format!(
        "\\PassOptionsToClass{{tikzexport}}{{ximera}}\\PassOptionsToClass{{xake}}{{ximera}}\
         \\PassOptionsToClass{{xake}}{{xourse}}\\nonstopmode\\input{{{filename}}}"
    );
    // Like xmlatex, a failing first pass is not fatal: make4ht compiles again.
    let _ = run(
        directory,
        "pdflatex",
        &["-file-line-error", "-shell-escape", &input],
    );
    run(
        directory,
        "make4ht",
        &[
            "-s",
            "-c",
            "ximera",
            "-f",
            "html5+dvisvgm_hashes",
            "-u",
            "-a",
            "debug",
            filename,
            "svg,htex4ht,mathjax,-css,info,tikz+",
            "",
            "",
            "--interaction=nonstopmode -shell-escape -file-line-error",
        ],
    )
}

fn run_docker(work: &Path, relative: &Path, filename: &str) -> Result<(), String> {
    let image = env::var("TRADUZ_XIMERA_IMAGE").unwrap_or_else(|_| DEFAULT_IMAGE.to_owned());
    // Docker bind mounts require an absolute source path.
    let work = work
        .canonicalize()
        .map_err(io_error("resolvendo a cópia temporária do curso"))?;
    let mount = format!("type=bind,source={},target=/code", work.display());
    let workdir = match relative.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(parent) => format!("/code/{}", parent.display()),
        None => "/code".to_owned(),
    };
    let output = Command::new("timeout")
        .arg(COMPILE_TIMEOUT)
        .arg("docker")
        .args(["run", "--rm", "--network", "none", "--mount"])
        .arg(mount)
        .args(["-w", &workdir])
        .arg(image)
        .args(["xmlatex", "texHtml", filename])
        .output()
        .map_err(io_error("executando o renderizador oficial do Ximera"))?;
    if !output.status.success() {
        return Err(command_error("ximeralatex", &output.stdout, &output.stderr));
    }
    Ok(())
}

fn run(directory: &Path, program: &str, arguments: &[&str]) -> Result<(), String> {
    let output = Command::new("timeout")
        .arg(COMPILE_TIMEOUT)
        .arg(program)
        .args(arguments)
        .current_dir(directory)
        .output()
        .map_err(io_error("executando o compilador do Ximera"))?;
    if !output.status.success() {
        return Err(command_error(program, &output.stdout, &output.stderr));
    }
    Ok(())
}

fn collect_images(html: &str, directory: &Path, work: &Path) -> Result<Vec<Image>, String> {
    let work = work
        .canonicalize()
        .map_err(io_error("resolvendo a cópia temporária do curso"))?;
    let mut images: Vec<Image> = Vec::new();
    for reference in image_references(html) {
        if images.iter().any(|image| image.reference == reference) {
            continue;
        }
        let Ok(candidate) = directory.join(&reference).canonicalize() else {
            continue;
        };
        if !candidate.starts_with(&work) || !candidate.is_file() {
            continue;
        }
        let Some(name) = candidate
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        let bytes = fs::read(&candidate).map_err(io_error("lendo imagem do Ximera"))?;
        images.push(Image {
            reference,
            name,
            bytes,
        });
    }
    Ok(images)
}

/// Local `src` values of `<img>` tags, in document order.
fn image_references(html: &str) -> Vec<String> {
    let mut references = Vec::new();
    for marker in ["src='", "src=\""] {
        let quote = marker.chars().last().expect("quote");
        let mut search = 0;
        while let Some(found) = html[search..].find(marker) {
            let start = search + found + marker.len();
            let Some(length) = html[start..].find(quote) else {
                break;
            };
            let value = &html[start..start + length];
            search = start + length + 1;
            if value.is_empty()
                || value.contains("://")
                || value.starts_with("data:")
                || value.starts_with('/')
            {
                continue;
            }
            references.push(value.to_owned());
        }
    }
    references
}

/// Correct answer of an `\answer` command, consumed by the reader script.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AnswerSpec {
    pub answer: String,
    pub format: String,
    pub given: bool,
    pub antiderivative: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<f64>,
}

impl AnswerSpec {
    fn new(options: &str, answer: String) -> AnswerSpec {
        let mut spec = AnswerSpec {
            answer,
            format: "expression".to_owned(),
            given: false,
            antiderivative: false,
            tolerance: None,
        };
        for option in options.split(',').map(str::trim) {
            match option.split_once('=') {
                Some(("format", value)) => spec.format = value.trim().to_owned(),
                Some(("tolerance", value)) => spec.tolerance = value.trim().parse().ok(),
                Some(_) => {}
                None if option == "given" => spec.given = true,
                None if option == "antiderivative" => spec.antiderivative = true,
                None => {}
            }
        }
        spec
    }
}

/// Turns the TeX4ht output into the fragment the reader embeds: the body
/// without scripts, with the MathJax preamble ximeraLatex collected, with
/// `\answer` boxes the reader script can fill in, and with image URLs that
/// point at the rendered tree.  Mirrors the clean-up luaxake performs when
/// publishing to a Ximera server.
fn finish(compiled: &Compiled, url_directory: &str, signature: &str) -> Result<String, String> {
    let body = activity_body(&compiled.html)?;
    let body = remove_elements(&body, "script");
    let body = remove_elements(&body, "style");
    let body = remove_comments(&body);
    let body = remove_preamble(&body);
    let body = body
        .replace("\\begin {", "\\begin{")
        .replace("\\end {", "\\end{")
        // Unnumbered problems carry a lone space that would print "Pergunta .".
        .replace("numbered=\" \"", "numbered=\"\"")
        .replace("numbered=' '", "numbered=''");
    let (mut body, answers) = transform_answers(&body);
    for image in &compiled.images {
        let url = format!("{url_directory}/{}", image.name);
        body = body
            .replace(
                &format!("src='{}'", image.reference),
                &format!("src=\"{url}\""),
            )
            .replace(
                &format!("src=\"{}\"", image.reference),
                &format!("src=\"{url}\""),
            );
    }
    let body = remove_empty_paragraphs(&body);
    let macros = mathjax_macros(&compiled.macros);
    let answers = serde_json::to_string(&answers).map_err(|error| error.to_string())?;
    let mut output = format!("{SIGNATURE_PREFIX} {signature} -->\n");
    if !macros.is_empty() {
        output.push_str("<div class=\"preamble\">\\(\n");
        output.push_str(&macros);
        output.push_str("\n\\)</div>\n");
    }
    output.push_str(body.trim());
    output.push_str("\n<script type=\"application/json\" class=\"xm-answers\">");
    output.push_str(&answers.replace("</", "<\\/"));
    output.push_str("</script>\n");
    Ok(output)
}

fn activity_body(html: &str) -> Result<String, String> {
    let body_start = html
        .find("<body")
        .and_then(|start| html[start..].find('>').map(|end| start + end + 1))
        .ok_or_else(|| "HTML do Ximera não contém body".to_string())?;
    let body_end = html[body_start..]
        .find("</body>")
        .map(|end| body_start + end)
        .unwrap_or(html.len());
    Ok(html[body_start..body_end].to_owned())
}

/// Removes every `<tag ...>...</tag>` element (not nested).
fn remove_elements(input: &str, tag: &str) -> String {
    let opening = format!("<{tag}");
    let closing = format!("</{tag}>");
    let mut result = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find(&opening) {
        let after = &rest[start + opening.len()..];
        if !after.starts_with(['>', ' ', '\n', '\t']) {
            result.push_str(&rest[..start + opening.len()]);
            rest = after;
            continue;
        }
        result.push_str(&rest[..start]);
        match after.find(&closing) {
            Some(end) => rest = &after[end + closing.len()..],
            None => return result,
        }
    }
    result.push_str(rest);
    result
}

fn remove_comments(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("<!--") {
        result.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            None => return result,
        }
    }
    result.push_str(rest);
    result
}

/// ximeraLatex opens the body with `<div class="preamble">` holding scripts;
/// the reader adds its own preamble with the MathJax macros instead.
fn remove_preamble(input: &str) -> String {
    let Some(start) = input
        .find("<div class=\"preamble\">")
        .or_else(|| input.find("<div class='preamble'>"))
    else {
        return input.to_owned();
    };
    let mut depth = 0usize;
    let mut at = start;
    while at < input.len() {
        if input[at..].starts_with("<div") {
            depth += 1;
            at += 4;
        } else if input[at..].starts_with("</div>") {
            depth -= 1;
            at += 6;
            if depth == 0 {
                return format!("{}{}", &input[..start], &input[at..]);
            }
        } else {
            at += input[at..].chars().next().map_or(1, char::len_utf8);
        }
    }
    input.to_owned()
}

fn remove_empty_paragraphs(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("<p") {
        let after = &rest[start + 2..];
        if !after.starts_with(['>', ' ', '\n']) {
            result.push_str(&rest[..start + 2]);
            rest = after;
            continue;
        }
        let Some(tag_end) = after.find('>') else {
            break;
        };
        let content_start = tag_end + 1;
        let Some(length) = after[content_start..].find("</p>") else {
            break;
        };
        let content = &after[content_start..content_start + length];
        if content.trim().is_empty() {
            result.push_str(&rest[..start]);
            rest = &after[content_start + length + 4..];
        } else {
            result.push_str(&rest[..start + 2 + content_start]);
            rest = &after[content_start..];
        }
    }
    result.push_str(rest);
    result
}

/// Keeps the definitions luaxake keeps from the `.xmjax` file: plain
/// `\newcommand`s, environments and math operators, without internal macros.
fn mathjax_macros(xmjax: &str) -> String {
    // TeX4ht internals that luaxake lets through but that MathJax can never
    // use; a single failing definition would abort the ones after it.
    const INTERNAL: [&str; 14] = [
        "\\HCode",
        "\\ifvmode",
        "\\ifthenelse",
        "\\csname",
        "\\expandafter",
        "\\NoFonts",
        "\\stepcounter",
        "\\refstepcounter",
        "\\renewenvironment",
        "\\def",
        "\\let",
        "\\global",
        "\\immediate",
        "\\write",
    ];
    xmjax
        .lines()
        .filter(|line| {
            !line.contains([':', '*', '@', '<', '>'])
                && !line.contains("\\_")
                && !line.contains("\\TU")
                && !line.contains("\\T1")
                && !line.contains("\\?")
                && !line.contains("\\label")
                && !INTERNAL.iter().any(|internal| line.contains(internal))
                && (line.starts_with("\\newcommand {")
                    || line.starts_with("\\DeclareMathOperator")
                    || line.starts_with("\\newenvironment"))
        })
        .map(|line| line.replace("##", "#"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Replaces `\answer[options]{tex}` inside mathematics with a labelled box the
/// reader script turns into an input field; `given` answers are shown as is.
fn transform_answers(body: &str) -> (String, BTreeMap<String, AnswerSpec>) {
    const COMMAND: &str = "\\answer";
    let mut result = String::with_capacity(body.len());
    let mut answers = BTreeMap::new();
    let mut rest = body;
    let mut count = 0usize;
    while let Some(found) = rest.find(COMMAND) {
        let after = &rest[found + COMMAND.len()..];
        if after
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic())
        {
            result.push_str(&rest[..found + COMMAND.len()]);
            rest = after;
            continue;
        }
        let Some((options, content, consumed)) = parse_answer(after) else {
            result.push_str(&rest[..found + COMMAND.len()]);
            rest = after;
            continue;
        };
        result.push_str(&rest[..found]);
        let spec = AnswerSpec::new(options, html_unescape(content));
        if spec.given {
            result.push_str("\\class{xm-given}{");
            result.push_str(content);
            result.push('}');
        } else {
            count += 1;
            let id = format!("{ANSWER_ID_PREFIX}{count}");
            result.push_str(&format!(
                "\\class{{xm-answer}}{{\\cssId{{{id}}}{{\\phantom{{\\rule{{7em}}{{1.5em}}}}}}}}"
            ));
            answers.insert(id, spec);
        }
        rest = &after[consumed..];
    }
    result.push_str(rest);
    (result, answers)
}

fn parse_answer(input: &str) -> Option<(&str, &str, usize)> {
    let mut at = skip_space(input, 0);
    let mut options = "";
    if input[at..].starts_with('[') {
        let end = input[at..].find(']')? + at;
        options = &input[at + 1..end];
        at = skip_space(input, end + 1);
    }
    if !input[at..].starts_with('{') {
        return None;
    }
    let end = balanced_end(input, at, '{', '}')?;
    Some((options, &input[at + 1..end - 1], end))
}

fn html_unescape(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn command_argument<'a>(source: &'a str, command: &str) -> Option<&'a str> {
    let needle = format!("\\{command}");
    let mut search = 0;
    while let Some(relative) = source[search..].find(&needle) {
        let command_start = search + relative;
        let mut cursor = command_start + needle.len();
        if source[cursor..]
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic())
        {
            search = cursor;
            continue;
        }
        cursor = skip_space(source, cursor);
        if source[cursor..].starts_with('[') {
            cursor = balanced_end(source, cursor, '[', ']')?;
            cursor = skip_space(source, cursor);
        }
        if source[cursor..].starts_with('{') {
            let end = balanced_end(source, cursor, '{', '}')?;
            return source.get(cursor + 1..end - 1);
        }
        search = cursor;
    }
    None
}

fn leading_prose(translation: &str) -> Option<&str> {
    let end = translation.find("{{").unwrap_or(translation.len());
    translation.get(..end)
}

fn abstract_prose<'a>(translation: &'a str, placeholders: &[Placeholder]) -> Option<&'a str> {
    let begin = placeholders
        .iter()
        .find(|placeholder| placeholder.original.contains("\\begin{abstract}"))?;
    let end = placeholders
        .iter()
        .find(|placeholder| placeholder.original.contains("\\end{abstract}"))?;
    let start = translation.find(&begin.token)? + begin.token.len();
    let end = translation[start..].find(&end.token)? + start;
    translation.get(start..end)
}

fn environment_body<'a>(source: &'a str, environment: &str) -> Option<&'a str> {
    let opening = format!("\\begin{{{environment}}}");
    let start = source.find(&opening)? + opening.len();
    let closing = format!("\\end{{{environment}}}");
    let end = source[start..].find(&closing)? + start;
    source.get(start..end)
}

fn skip_space(input: &str, mut at: usize) -> usize {
    while let Some(character) = input[at..].chars().next() {
        if !character.is_whitespace() {
            break;
        }
        at += character.len_utf8();
    }
    at
}

pub(crate) fn balanced_end(input: &str, start: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    let mut at = start;
    while let Some(character) = input[at..].chars().next() {
        if character == open {
            depth += 1;
        } else if character == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(at + character.len_utf8());
            }
        }
        at += character.len_utf8();
    }
    None
}

/// Prose of a LaTeX fragment without commands, braces and tokens.
pub(crate) fn plain_text(value: &str) -> Option<String> {
    let mut output = String::new();
    let mut in_space = false;
    let mut at = 0;
    while let Some(character) = value[at..].chars().next() {
        if value[at..].starts_with("{{") {
            if let Some(end) = value[at + 2..].find("}}") {
                at += end + 4;
                continue;
            }
        }
        if character == '%' {
            // A LaTeX comment runs to the end of the line.
            at += value[at..].find('\n').unwrap_or(value.len() - at);
            continue;
        }
        if value[at..].starts_with("``") {
            output.push('\u{201c}');
            in_space = false;
            at += 2;
            continue;
        }
        if value[at..].starts_with("''") {
            output.push('\u{201d}');
            in_space = false;
            at += 2;
            continue;
        }
        if character == '\\' {
            if value[at + 1..].starts_with('%') {
                output.push('%');
                in_space = false;
                at += 2;
                continue;
            }
            at += character.len_utf8();
            while let Some(command_character) = value[at..].chars().next() {
                if !(command_character.is_ascii_alphabetic()
                    || command_character == '@'
                    || command_character == '*')
                {
                    break;
                }
                at += command_character.len_utf8();
            }
            continue;
        } else if character == '{' || character == '}' {
        } else if character.is_whitespace() {
            if !in_space {
                output.push(' ');
                in_space = true;
            }
        } else {
            output.push(character);
            in_space = false;
        }
        at += character.len_utf8();
    }
    let output = output.trim();
    (!output.is_empty()).then(|| output.to_owned())
}

fn command_error(command: &str, stdout: &[u8], stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    let stdout = String::from_utf8_lossy(stdout);
    let lines: Vec<&str> = stderr
        .lines()
        .chain(stdout.lines())
        .filter(|line| !line.trim().is_empty())
        .collect();
    let details = lines
        .iter()
        .rev()
        .find(|line| line.contains('!') || line.contains("rror") || line.contains("ERROR"))
        .or_else(|| lines.last())
        .copied()
        .unwrap_or("sem detalhes");
    format!("{command} falhou: {}", details.trim())
}

fn io_error(action: &'static str) -> impl FnOnce(std::io::Error) -> String {
    move |error| format!("erro ao {action}: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Placeholder, PlaceholderKind};

    #[test]
    fn extracts_title_and_abstract_from_ximera_source() {
        let source = r"\title{Como usar o Ximera}\begin{document}\begin{abstract}Esse curso é construído com Ximera.\end{abstract}\maketitle";
        assert_eq!(
            metadata(source, &[]).title,
            Some("Como usar o Ximera".to_owned())
        );
        assert_eq!(
            metadata(source, &[]).summary,
            Some("Esse curso é construído com Ximera.".to_owned())
        );
    }

    #[test]
    fn extracts_metadata_when_the_title_opening_is_outside_the_segment() {
        let placeholders = vec![
            Placeholder {
                token: "{{CMD_1}}".into(),
                original: "}".into(),
                kind: PlaceholderKind::Command,
            },
            Placeholder {
                token: "{{PREAMBLE_2}}".into(),
                original: "\n\n".into(),
                kind: PlaceholderKind::Preamble,
            },
            Placeholder {
                token: "{{CMD_3}}".into(),
                original: "\\begin{document}".into(),
                kind: PlaceholderKind::Command,
            },
            Placeholder {
                token: "{{CMD_4}}".into(),
                original: "\\begin{abstract}".into(),
                kind: PlaceholderKind::Command,
            },
            Placeholder {
                token: "{{CMD_5}}".into(),
                original: "\\end{abstract}".into(),
                kind: PlaceholderKind::Command,
            },
            Placeholder {
                token: "{{CMD_6}}".into(),
                original: "\\maketitle".into(),
                kind: PlaceholderKind::Command,
            },
        ];
        let activity = metadata(
            "Como usar o Ximera{{CMD_1}}{{PREAMBLE_2}}{{CMD_3}}\n{{CMD_4}}Esse curso é construído com Ximera.{{CMD_5}}{{CMD_6}}",
            &placeholders,
        );
        assert_eq!(activity.title.as_deref(), Some("Como usar o Ximera"));
        assert_eq!(
            activity.summary.as_deref(),
            Some("Esse curso é construído com Ximera.")
        );
    }

    #[test]
    fn plain_text_drops_comments_and_typesets_quotes() {
        assert_eq!(
            plain_text("Aqui ``desfazemos'' 100\\% %Stuff can go here\nfunções."),
            Some("Aqui \u{201c}desfazemos\u{201d} 100% funções.".to_owned())
        );
        assert_eq!(plain_text("%só comentário"), None);
    }

    #[test]
    fn recognizes_standalone_activities() {
        assert!(is_activity("\\documentclass{ximera}\n\\begin{document}"));
        assert!(is_activity("\\documentclass[handout]{ximera}"));
        assert!(!is_activity("\\documentclass{xourse}\n\\activity{a.tex}"));
        assert!(!is_activity("\\usepackage{esint}"));
    }

    #[test]
    fn answers_become_labelled_boxes_and_given_answers_stay_visible() {
        let (html, answers) = transform_answers(
            "\\(3\\times 2 = \\answer {6}\\) e \\(n=\\answer [given]{8}\\) e \
             \\(\\answer[format=string, tolerance=.2]{\\frac {a}{b}}\\) \\answers",
        );
        assert!(html.contains(
            "\\(3\\times 2 = \\class{xm-answer}{\\cssId{xm-answer-1}{\\phantom{\\rule{7em}{1.5em}}}}\\)"
        ));
        assert!(html.contains("\\(n=\\class{xm-given}{8}\\)"));
        assert!(html.ends_with("\\answers"));
        assert_eq!(answers.len(), 2);
        assert_eq!(answers["xm-answer-1"].answer, "6");
        assert_eq!(answers["xm-answer-1"].format, "expression");
        let second = &answers["xm-answer-2"];
        assert_eq!(second.answer, "\\frac {a}{b}");
        assert_eq!(second.format, "string");
        assert_eq!(second.tolerance, Some(0.2));
    }

    #[test]
    fn answer_text_is_unescaped_for_the_script() {
        let (_, answers) = transform_answers("\\(\\answer{x &lt; 2 &amp; y}\\)");
        assert_eq!(answers["xm-answer-1"].answer, "x < 2 & y");
    }

    #[test]
    fn keeps_only_plain_definitions_from_the_mathjax_preamble() {
        let macros = mathjax_macros(
            "\\newenvironment {prompt}{}{}\n\
             \\newcommand {\\dd }[2][]{\\frac {d ##1}{d ##2}}\n\
             \\newcommand {\\begin  }[1]{\\csname @begin:##1\\endcsname }\n\
             \\newcommand {\\_  }[0]{\\mathunderscore }\n\
             \\DeclareMathOperator {\\arcsec }{arcsec}\n\
             \\newcommand {\\foldable }[0]{\\stepcounter {identification}\\HCode {<div>}}\n\
             \\def\\foo{bar}",
        );
        assert_eq!(
            macros,
            "\\newenvironment {prompt}{}{}\n\
             \\newcommand {\\dd }[2][]{\\frac {d #1}{d #2}}\n\
             \\DeclareMathOperator {\\arcsec }{arcsec}"
        );
    }

    #[test]
    fn cleans_the_tex4ht_body() {
        let html = "<html><head><title>x</title></head><body>\n<div class=\"preamble\">\n<script>a</script>\n</div><style>p{}</style>\n<!--l. 10--><p class=\"noindent\" >Texto \\begin {align*} x \\end {align*}</p><p class='noindent'>\n</p><div class=\"theorem-like example\" numbered=\" \"><img src='fig-1.svg' alt=\"[Picture]\"/></div></body></html>";
        let compiled = Compiled {
            html: html.to_owned(),
            macros: String::new(),
            images: vec![Image {
                reference: "fig-1.svg".into(),
                name: "fig-1.svg".into(),
                bytes: Vec::new(),
            }],
        };
        let output = finish(&compiled, "/rendered/cap", "abc").unwrap();
        assert!(output.starts_with("<!-- traduz-render: abc -->\n"));
        assert_eq!(cached_signature(&output), Some("abc"));
        assert!(!output.contains("<script>a</script>"));
        assert!(!output.contains("preamble"));
        assert!(!output.contains("<style>"));
        assert!(!output.contains("<!--l. 10-->"));
        assert!(output.contains("\\begin{align*} x \\end{align*}"));
        assert!(output.contains("numbered=\"\""));
        assert!(!output.contains("<p class='noindent'>\n</p>"));
        assert!(output.contains("src=\"/rendered/cap/fig-1.svg\""));
        assert!(output
            .ends_with("<script type=\"application/json\" class=\"xm-answers\">{}</script>\n"));
    }

    #[test]
    fn preamble_macros_are_embedded_for_mathjax() {
        let compiled = Compiled {
            html: "<body><p>\\(\\dd{y}{x}\\)</p></body>".to_owned(),
            macros: "\\newcommand {\\dd }[2][]{\\frac {d ##1}{d ##2}}\n".to_owned(),
            images: Vec::new(),
        };
        let output = finish(&compiled, "/rendered", "s").unwrap();
        assert!(output.contains(
            "<div class=\"preamble\">\\(\n\\newcommand {\\dd }[2][]{\\frac {d #1}{d #2}}\n\\)</div>"
        ));
    }

    #[test]
    fn image_references_skip_external_sources() {
        let references = image_references(
            "<img src='a.svg'/> <img src=\"b.png\"> <img src='https://x/y.png'> <img src='/abs.png'> <img src='data:image/png;base64,AA'>",
        );
        assert_eq!(references, vec!["a.svg".to_owned(), "b.png".to_owned()]);
    }
}
