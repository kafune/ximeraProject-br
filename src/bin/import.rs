use clap::Parser;
use std::path::PathBuf;
#[derive(Parser)]
struct Args {
    #[arg(long)]
    db: PathBuf,
    #[arg(long)]
    course: String,
    #[arg(long)]
    source: PathBuf,
    #[arg(long, help = "Substitui somente arquivos cuja fonte mudou")]
    replace: bool,
    #[arg(long, help = "Arquivo mestre que define a ordem pedagógica")]
    manifest: Option<PathBuf>,
}
fn main() -> anyhow::Result<()> {
    let a = Args::parse();
    let r = traduz::importer::import_tree_with_manifest(
        &a.db,
        &a.course,
        &a.source,
        a.replace,
        a.manifest.as_deref(),
    )?;
    println!(
        "Importados: {}; inalterados: {}; segmentos: {}",
        r.imported, r.unchanged, r.segments
    );
    if let Some(path) = r.backup {
        println!("Backup consistente criado em {}", path.display());
    }
    Ok(())
}
