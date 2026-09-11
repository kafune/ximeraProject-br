use clap::Parser;
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use traduz::{db, routes};
#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "data/traduz.sqlite3")]
    db: PathBuf,
    #[arg(long, default_value = "127.0.0.1:3000")]
    listen: SocketAddr,
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let a = Args::parse();
    let conn = db::open(&a.db)?;
    let app = routes::app(Arc::new(Mutex::new(conn)));
    let listener = tokio::net::TcpListener::bind(a.listen).await?;
    eprintln!("Traduz em http://{}", a.listen);
    axum::serve(listener, app).await?;
    Ok(())
}
