use clap::Parser;
use std::{collections::VecDeque, path::PathBuf, sync::Mutex, thread, time::Instant};
use traduz::{
    db,
    ximera::{self, Outcome, Renderer},
};

/// Compila as atividades com o ximeraLatex e grava o HTML em `rendered/`.
#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "data/traduz.sqlite3")]
    db: PathBuf,
    #[arg(long, default_value_t = 2, help = "Compilações simultâneas")]
    jobs: usize,
    #[arg(long, help = "Só arquivos cujo caminho começa com este prefixo")]
    only: Option<String>,
    #[arg(long, help = "Recompila mesmo quando a página gravada já está atual")]
    force: bool,
    #[arg(long, help = "Inclui arquivos que não são atividades Ximera")]
    all: bool,
    #[arg(
        long,
        default_value = "auto",
        value_parser = ["auto", "local", "docker"],
        help = "Compilador: pdflatex+make4ht locais ou a imagem Docker oficial"
    )]
    renderer: String,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let conn = db::open(&args.db)?;
    let protected = db::protected_translations(&conn)?;
    let mut jobs = VecDeque::new();
    for file in db::file_records(&conn)? {
        if args
            .only
            .as_deref()
            .is_some_and(|prefix| !file.path.starts_with(prefix))
        {
            continue;
        }
        if !args.all && !ximera::is_activity(&file.source) {
            continue;
        }
        let segments = db::segments_for_file(&conn, file.id)?;
        jobs.push_back((file, segments));
    }
    drop(conn);
    let renderer = match args.renderer.as_str() {
        "local" => Renderer::Local,
        "docker" => Renderer::Docker,
        _ => Renderer::detect(),
    };
    let total = jobs.len();
    eprintln!(
        "Renderizador: {}; {} arquivos; {} em paralelo; saída em {}",
        renderer.name(),
        total,
        args.jobs.max(1),
        ximera::rendered_root().display()
    );
    let queue = Mutex::new(jobs);
    let compiled = Mutex::new(0usize);
    let cached = Mutex::new(0usize);
    let failures = Mutex::new(Vec::new());
    let started = Instant::now();
    thread::scope(|scope| {
        for _ in 0..args.jobs.max(1) {
            scope.spawn(|| loop {
                let next = queue.lock().expect("fila").pop_front();
                let Some((file, segments)) = next else { break };
                match ximera::render_file(&file, &segments, &protected, renderer, args.force) {
                    Ok(Outcome::Cached) => *cached.lock().expect("contador") += 1,
                    Ok(Outcome::Compiled) => {
                        *compiled.lock().expect("contador") += 1;
                        eprintln!("ok   {}", file.path);
                    }
                    Err(error) => {
                        eprintln!("ERRO {}: {error}", file.path);
                        failures.lock().expect("erros").push(file.path.clone());
                    }
                }
            });
        }
    });
    let failures = failures.into_inner().expect("erros");
    println!(
        "Compilados: {}; já atuais: {}; erros: {}; tempo: {:.0}s",
        compiled.into_inner().expect("contador"),
        cached.into_inner().expect("contador"),
        failures.len(),
        started.elapsed().as_secs_f64()
    );
    if !failures.is_empty() {
        for path in &failures {
            println!("  {path}");
        }
        std::process::exit(1);
    }
    Ok(())
}
