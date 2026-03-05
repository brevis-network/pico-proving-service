use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use dotenvy::dotenv;
use pico_proving_service::{
    prover_network_client::ProverNetworkClient, EstimateCostRequest, GetProvingResultRequest,
    ProveTaskRequest, RegisterAppRequest, VerifyProofRequest,
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

    #[command(about = "Verify an embed proof")]
    VerifyProof(VerifyProofCommand),
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

    #[arg(
        long,
        help = "Output file path to save proof (default: proof_{task_id}.bin)"
    )]
    output: Option<PathBuf>,
}

#[derive(Args)]
struct VerifyProofCommand {
    #[arg(long, help = "Application unique ID")]
    app_id: String,

    #[arg(long, help = "Proof file path")]
    proof: PathBuf,
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
                app_id: cmd.app_id.clone(),
                task_id: cmd.task_id.clone(),
            };
            let res = client.get_proving_result(req).await?.into_inner();

            // Save proof to file if it exists
            if let Some(proof) = res.proof {
                let output_path = cmd
                    .output
                    .unwrap_or_else(|| PathBuf::from(format!("proof_{}.bin", cmd.task_id)));
                fs::write(&output_path, &proof)?;
                info!(
                    "GetProvingResult: err={:?}, proof saved to {:?} ({} bytes)",
                    res.err,
                    output_path,
                    proof.len()
                );
            } else {
                info!("GetProvingResult: err={:?}, proof=None", res.err);
            }
        }
        Command::VerifyProof(cmd) => {
            let proof = fs::read(&cmd.proof)?;

            let req = VerifyProofRequest {
                app_id: cmd.app_id,
                proof,
            };
            let res = client.verify_proof(req).await?.into_inner();

            info!("VerifyProof: err={:?}, verified={}", res.err, res.verified);
        }
    }

    Ok(())
}
