use anyhow::Result;
use clap::Parser;
use dotenvy::dotenv;
use pico_proving_service::app_manager::App;
use pico_vm::machine::logger::setup_logger;
use std::{fs, path::PathBuf};
use tracing::info;

#[derive(Parser)]
struct Cli {
    #[arg(long, help = "Application ELF file path")]
    elf: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    setup_logger();

    let cli = Cli::parse();
    let elf = fs::read(cli.elf)?;

    let app = App::new(&elf, None);
    let app_id = app.app_id;

    info!("Generated app_id: 0x{app_id}");

    Ok(())
}
