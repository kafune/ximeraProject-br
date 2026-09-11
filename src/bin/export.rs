use clap::Parser;
use std::path::PathBuf;
#[derive(Parser)]
struct Args {
    #[arg(long)]
    db: PathBuf,
    #[arg(long)]
    source: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    require_complete: bool,
}
fn main() -> anyhow::Result<()> {
    let a = Args::parse();
    let r = traduz::export::export_tree(&a.db, &a.source, &a.output, a.require_complete)?;
    println!(
        "Arquivos: {}; concluídos: {}; pendentes: {} (rascunhos exportados como original: {})",
        r.files, r.completed, r.pending, r.drafts
    );
    Ok(())
}
