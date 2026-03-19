use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use dotenvy::dotenv;
use pico_proving_service::{
    EstimateCostRequest, GetProvingResultRequest, ProveTaskRequest, RegisterAppRequest,
    prover_network_client::ProverNetworkClient,
};
use pico_vm::machine::logger::setup_logger;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::PathBuf,
};
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

    #[command(about = "Batch estimate cost for multiple inputs and output CSV")]
    BatchEstimateCost(BatchEstimateCostCommand),

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

    #[arg(long, help = "Treat inputs as raw bytes (use write_slice mode)")]
    raw_input: bool,
}

#[derive(Args)]
struct BatchEstimateCostCommand {
    #[arg(long, help = "Application unique ID")]
    app_id: String,

    #[arg(long, help = "Directory containing input .bin files")]
    input_dir: PathBuf,

    #[arg(long, default_value = "*.bin", help = "File name prefix filter (e.g. '24000')")]
    prefix: String,

    #[arg(long, default_value = "precompile_counts.csv", help = "Output CSV file path")]
    output: PathBuf,
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
                raw_input: if cmd.raw_input { Some(true) } else { None },
            };
            let res = client.estimate_cost(req).await?.into_inner();

            info!(
                "EstimateCost: err={:?}, cost={}, pv_digest={:?}, precompile_counts={:?}",
                res.err, res.cost, res.pv_digest, res.precompile_counts
            );
        }
        Command::BatchEstimateCost(cmd) => {
            // Collect input files
            let mut input_files: Vec<PathBuf> = fs::read_dir(&cmd.input_dir)?
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    let path = entry.path();
                    if path.is_file() {
                        let name = path.file_name()?.to_str()?;
                        if name.ends_with(".bin") && (cmd.prefix == "*.bin" || name.starts_with(&cmd.prefix)) {
                            return Some(path);
                        }
                    }
                    None
                })
                .collect();
            input_files.sort();

            info!("Found {} input files in {:?}", input_files.len(), cmd.input_dir);

            // Fixed column order for CSV (all known precompile syscall codes)
            let precompile_columns = [
                "SHA_COMPRESS", "SHA_EXTEND", "KECCAK_PERMUTE", "POSEIDON2_PERMUTE",
                "ED_ADD", "ED_DECOMPRESS",
                "SECP256K1_ADD", "SECP256K1_DOUBLE", "SECP256K1_DECOMPRESS",
                "SECP256R1_ADD", "SECP256R1_DOUBLE", "SECP256R1_DECOMPRESS",
                "BN254_ADD", "BN254_DOUBLE",
                "BLS12381_ADD", "BLS12381_DOUBLE", "BLS12381_DECOMPRESS",
                "BN254_FP_ADD", "BN254_FP_SUB", "BN254_FP_MUL",
                "BN254_FP2_ADD", "BN254_FP2_SUB", "BN254_FP2_MUL",
                "BLS12381_FP_ADD", "BLS12381_FP_SUB", "BLS12381_FP_MUL",
                "BLS12381_FP2_ADD", "BLS12381_FP2_SUB", "BLS12381_FP2_MUL",
                "SECP256K1_FP_ADD", "SECP256K1_FP_SUB", "SECP256K1_FP_MUL",
                "UINT256_MUL",
            ];

            // Write CSV header immediately
            let mut csv_file = fs::File::create(&cmd.output)?;
            write!(csv_file, "block,cost")?;
            for col in &precompile_columns {
                write!(csv_file, ",{}", col)?;
            }
            writeln!(csv_file)?;

            // Process each input and append row immediately
            let mut completed = 0usize;
            for (i, input_path) in input_files.iter().enumerate() {
                let block_name = input_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                info!("[{}/{}] Processing block: {}", i + 1, input_files.len(), block_name);

                let inputs = fs::read(input_path)?;

                let req = EstimateCostRequest {
                    app_id: cmd.app_id.clone(),
                    inputs: Some(inputs),
                    raw_input: Some(true),
                };

                let res = client.estimate_cost(req).await?.into_inner();

                if let Some(ref err) = res.err {
                    info!("  Error for {}: {:?}", block_name, err);
                }

                info!(
                    "  cost={}, precompile_counts={:?}",
                    res.cost, res.precompile_counts
                );

                // Append row to CSV immediately
                write!(csv_file, "{},{}", block_name, res.cost)?;
                for col in &precompile_columns {
                    let count = res.precompile_counts.get(*col).unwrap_or(&0);
                    write!(csv_file, ",{}", count)?;
                }
                writeln!(csv_file)?;
                csv_file.flush()?;

                completed += 1;
            }

            info!(
                "CSV written to {:?} ({} blocks completed)",
                cmd.output, completed
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
