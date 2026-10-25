use clap::Parser;
use std::path::PathBuf;
#[derive(Parser)]
#[command(about = "Aplica ao banco traduções guardadas em JSON (ver translations/)")]
struct Args {
    #[arg(long)]
    db: PathBuf,
    #[arg(long, help = "Arquivo .json ou diretório com arquivos .json")]
    translations: PathBuf,
    #[arg(
        long,
        help = "Substitui também segmentos que já têm rascunho ou tradução; sem esta opção, só pendentes recebem texto"
    )]
    overwrite: bool,
}
fn main() -> anyhow::Result<()> {
    let a = Args::parse();
    let protected = traduz::translations::load_protected_entries(&a.translations)?;
    if !protected.is_empty() {
        let n = traduz::translations::apply_protected_entries(&a.db, &protected)?;
        println!("Trechos protegidos registrados: {n}");
    }
    let entries = traduz::translations::load_entries(&a.translations)?;
    let r = traduz::translations::apply_entries(&a.db, &entries, a.overwrite)?;
    println!(
        "Entradas: {}; aplicadas: {}; já iguais: {}; mantidas (tinham texto): {}",
        r.entries, r.applied, r.identical, r.kept
    );
    for label in &r.missing {
        println!("Aviso: {label} não existe no banco");
    }
    for label in &r.hash_mismatch {
        println!("Aviso: {label} mudou desde a tradução; entrada ignorada");
    }
    for (label, error) in &r.invalid {
        println!("Erro: {label}: {error}");
    }
    if r.has_problems() {
        std::process::exit(2);
    }
    Ok(())
}
