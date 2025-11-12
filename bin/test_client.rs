use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use dotenvy::dotenv;
use pico_proving_service::{
    EstimateCostRequest, GetProvingResultRequest, ProveTaskRequest, RegisterAppRequest,
    prover_network_client::ProverNetworkClient,
};
use pico_vm::machine::logger::setup_logger;
use std::{fs, path::PathBuf};
use tonic::codec::CompressionEncoding;
use tracing::info;

#[derive(Parser)]
struct Cli {
    #[clap(
        long,
        env = "GRPC_ADDR",
        default_value = "http://[::]:50052",
        help = "gRPC address"
    )]
    pub grpc_addr: String,

    #[clap(
        long,
        env = "MAX_GRPC_MSG_SIZE",
        default_value = "1073741824",
        help = "Max gRPC message size (bytes)"
    )]
    pub max_grpc_msg_size: usize,

    #[command(subcommand)]
    pub cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Register a new application with build elf")]
    RegisterApp(RegisterAppCommand),

    #[command(about = "Estimate gas cost for an application")]
    EstimateCost(EstimateCostCommand),

    #[command(about = "Add a proving task")]
    ProveTask(ProveTaskCommand),

    #[command(about = "Fetch the proving result if complete")]
    GetProvingResult(GetProvingResultCommand),
}

#[derive(Args)]
struct RegisterAppCommand {
    #[arg(long, help = "Application ELF file path")]
    elf: PathBuf,

    #[arg(long, help = "Application information")]
    info: Option<String>,
}

#[derive(Args)]
struct EstimateCostCommand {
    #[arg(long, help = "Application unique ID")]
    app_id: String,

    #[arg(long, help = "Input file paths")]
    inputs: Option<PathBuf>,
}

#[derive(Args)]
struct ProveTaskCommand {
    #[arg(long, help = "Application unique ID")]
    app_id: String,

    #[arg(long, help = "Proving task unique ID")]
    task_id: String,

    #[arg(long, help = "Input file paths")]
    inputs: Option<PathBuf>,

    #[arg(long, help = "Use GPU for proving (default: false, use CPU)")]
    use_gpu: bool,
}

#[derive(Args)]
struct GetProvingResultCommand {
    #[arg(long, help = "Application unique ID")]
    app_id: String,

    #[arg(long, help = "Proving task unique ID")]
    task_id: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    setup_logger();

    let cli = Cli::parse();

    let mut client = ProverNetworkClient::connect(cli.grpc_addr.clone())
        .await?
        .max_encoding_message_size(cli.max_grpc_msg_size)
        .max_decoding_message_size(cli.max_grpc_msg_size)
        .accept_compressed(CompressionEncoding::Zstd)
        .send_compressed(CompressionEncoding::Zstd);

    match cli.cmd {
        Command::RegisterApp(cmd) => {
            let elf = fs::read(cmd.elf)?;

            let req = RegisterAppRequest {
                elf,
                info: cmd.info,
            };
            let res = client.register_app(req).await?.into_inner();

            info!("RegisterApp: err={:?}", res.err);
        }
        Command::EstimateCost(cmd) => {
            let inputs = if let Some(file_path) = cmd.inputs {
                Some(fs::read(file_path)?)
            } else {
                None
            };

            let req = EstimateCostRequest {
                app_id: cmd.app_id,
                inputs,
            };
            let res = client.estimate_cost(req).await?.into_inner();

            info!(
                "EstimateCost: err={:?}, cost={}, pv_digest={:?}",
                res.err, res.cost, res.pv_digest
            );
        }
        Command::ProveTask(cmd) => {
            let inputs = if let Some(file_path) = cmd.inputs {
                Some(fs::read(file_path)?)
            } else {
                None
            };

            let req = ProveTaskRequest {
                app_id: cmd.app_id,
                task_id: cmd.task_id,
                inputs,
                use_gpu: Some(cmd.use_gpu),
            };
            let res = client.prove_task(req).await?.into_inner();

            info!("ProveTask: err={:?}", res.err);
        }
        Command::GetProvingResult(cmd) => {
            let req = GetProvingResultRequest {
                app_id: cmd.app_id,
                task_id: cmd.task_id,
            };
            let res = client.get_proving_result(req).await?.into_inner();

            info!("GetProvingResult: err={:?}, proof={:?}", res.err, res.proof);
        }
    }

    Ok(())
}
