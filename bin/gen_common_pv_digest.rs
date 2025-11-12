use anyhow::Result;
use clap::Parser;
use dotenvy::dotenv;
use pico_proving_service::{app_manager::App, cost_estimation::estimate_cost};
use pico_vm::machine::logger::setup_logger;
use std::{fs, path::PathBuf};
use tracing::info;

#[derive(Parser)]
struct Cli {
    #[arg(long, help = "Application ELF file path")]
    elf: PathBuf,

    #[arg(long, help = "Input file paths")]
    inputs: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    setup_logger();

    let cli = Cli::parse();
    let elf = fs::read(cli.elf)?;
    let inputs = if let Some(file_path) = cli.inputs {
        Some(fs::read(file_path)?)
    } else {
        None
    };

    let app = App::new(&elf, None);
    let info = estimate_cost(app.program, app.pk, app.vk, inputs.as_deref(), None, false)?;

    let cycles = info.total_cycles;
    info!("Emulation cycles: {cycles}");

    let pv_digest = info.pv_digest;
    let pv_digest = format!("0x{:064x}", pv_digest);
    info!("Generated pv_digest: {pv_digest}");

    Ok(())
}
