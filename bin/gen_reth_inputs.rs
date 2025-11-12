use anyhow::Result;
use clap::Parser;
use dotenvy::dotenv;
use pico_proving_service::{app_manager::App, cost_estimation::estimate_cost, types::SC};
use pico_vm::{
    compiler::riscv::program::Program, emulator::stdin::EmulatorStdin,
    machine::logger::setup_logger,
};
use rsp_host_executor::EthHostExecutor;
use rsp_primitives::{chain_spec, genesis::Genesis};
use rsp_provider::create_provider;
use std::{
    fs,
    path::{Path, PathBuf},
};
use url::Url;

#[derive(Parser)]
struct Cli {
    #[arg(
        long,
        help = "Block number to generate reth inputs and public values digest"
    )]
    block_number: u64,

    #[arg(long, default_value = "fixtures/reth-elf", help = "reth ELF file path")]
    elf: PathBuf,

    #[clap(
        long,
        default_value = ".",
        help = "Base directory for saving files of reth inputs and public values digest: \
reth_input_BLOCK_NUMBER.bin and reth_pv_digest_BLOCK_NUMBER.bin"
    )]
    dump_dir: PathBuf,

    #[clap(long, env = "PICO_RPC_URL", help = "HTTP RPC URL")]
    rpc_url: Url,
}

#[tokio::main]
async fn main() -> Result<()> {
    // setup env and logger
    dotenv().ok();
    setup_logger();

    // parse cli
    let cli = Cli::parse();
    let block_number = cli.block_number;
    let elf = cli.elf;
    let dump_dir = cli.dump_dir;
    let rpc_url = cli.rpc_url;

    // create the dump parent dir
    fs::create_dir_all(&dump_dir)?;

    // generate inputs
    let inputs = generate_inputs(block_number, rpc_url).await?;

    // save `reth_input_BLOCK_NUMBER.bin`
    let input_path = dump_dir.join(format!("reth_input_{block_number}.bin"));
    fs::write(input_path, &inputs)?;

    // generate public values digest
    let pv_digest = generate_pv_digest(&elf, &inputs)?;

    // save `reth_pv_digest_BLOCK_NUMBER.bin`
    let pv_digest_path = dump_dir.join(format!("reth_pv_digest_{block_number}.bin"));
    fs::write(pv_digest_path, pv_digest)?;

    Ok(())
}

async fn generate_inputs(block_number: u64, rpc_url: Url) -> Result<Vec<u8>> {
    // create the rpc provider
    let rpc_provider = create_provider(rpc_url);

    // create the executor
    let chain_spec = chain_spec::mainnet()?.into();
    let executor = EthHostExecutor::eth(chain_spec, None);

    // execute to generate reth client input
    let input = executor
        .execute(
            block_number,
            &rpc_provider,
            Genesis::Mainnet,
            None,
            false,
            &None,
        )
        .await?;

    // write the input into stdin builder
    let mut stdin_builder = EmulatorStdin::<Program, Vec<u8>>::new_builder::<SC>();
    stdin_builder.write(&input);

    // serialize the stdin builder
    Ok(bincode::serialize(&stdin_builder)?)
}

fn generate_pv_digest(elf_file_path: &Path, inputs: &[u8]) -> Result<String> {
    // read reth elf
    let elf = fs::read(elf_file_path)?;

    let app = App::new(&elf, None);

    let info = estimate_cost(app.program, app.pk, app.vk, Some(inputs), None, false)?;
    let pv_digest = info.pv_digest;

    Ok(format!("0x{pv_digest:064x}"))
}
