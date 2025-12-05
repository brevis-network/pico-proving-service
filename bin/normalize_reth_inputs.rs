use anyhow::Result;
use clap::Parser;
use dotenvy::dotenv;
use pico_proving_service::types::SC;
use pico_vm::{
    compiler::riscv::program::Program, emulator::stdin::EmulatorStdin,
    machine::logger::setup_logger,
};
use rsp_client_executor::io::EthClientExecutorInput;
use std::{fs::File, path::PathBuf};

#[derive(Parser)]
struct Cli {
    #[arg(short, long, help = "RSP input file path")]
    input: PathBuf,

    #[arg(short, long, help = "Normalized output file path")]
    output: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    // setup env and logger
    dotenv().ok();
    setup_logger();

    // parse cli
    let cli = Cli::parse();

    // load eth client input from input file
    let mut input_file = File::open(cli.input)?;
    let client_input: EthClientExecutorInput = bincode::deserialize_from(&mut input_file)?;

    // write client input into stdin builder
    let mut stdin_builder = EmulatorStdin::<Program, Vec<u8>>::new_builder::<SC>();
    stdin_builder.write(&client_input);

    // store stdin builder into output file
    let mut output_file = File::create(cli.output)?;
    bincode::serialize_into(&mut output_file, &stdin_builder)?;

    Ok(())
}
