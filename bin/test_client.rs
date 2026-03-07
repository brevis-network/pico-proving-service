use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use dotenvy::dotenv;
use pico_proving_service::{
    EstimateCostRequest, GetProvingResultRequest, ProveTaskRequest, ProveVeraTaskRequest,
    RegisterAppRequest, Transform, VeraInput, VeraOutput, VerifyVeraProofRequest,
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

    #[command(about = "Verify an embed proof")]
    VerifyProof(VerifyProofCommand),

    #[command(about = "Submit a Vera proving task")]
    ProveVera(ProveVeraCommand),

    #[command(about = "Verify a Vera proof")]
    VerifyVera(VerifyVeraCommand),
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

    #[arg(long, help = "GuestOutput JSON file path")]
    output: PathBuf,
}

#[derive(Args)]
struct ProveVeraCommand {
    #[arg(long, help = "Application unique ID")]
    app_id: String,

    #[arg(long, help = "Proving task unique ID")]
    task_id: String,

    #[arg(long, help = "GuestInput JSON file path")]
    input: PathBuf,

    #[arg(
        long,
        help = "Output file path to save proof (default: proof_{task_id}.bin)"
    )]
    output: Option<PathBuf>,

    #[arg(
        long,
        help = "Output file path to save VeraOutput JSON (default: guest_output_{task_id}.json)"
    )]
    guest_output: Option<PathBuf>,
}

#[derive(Args)]
struct VerifyVeraCommand {
    #[arg(long, help = "Application unique ID")]
    app_id: String,

    #[arg(long, help = "Proof file path")]
    proof: PathBuf,

    #[arg(long, help = "GuestOutput JSON file path")]
    output: PathBuf,
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
                guest_output: None,
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
            let output_json = fs::read_to_string(&cmd.output)?;
            let vera_output = parse_guest_output_to_proto(&output_json)?;

            let req = VerifyVeraProofRequest {
                app_id: cmd.app_id,
                proof,
                output: Some(vera_output),
            };
            let res = client.verify_vera_proof(req).await?.into_inner();

            info!("VerifyProof: err={:?}, verified={}", res.err, res.verified);
        }
        Command::ProveVera(cmd) => {
            let input_json = fs::read_to_string(&cmd.input)?;
            let vera_input = parse_guest_input_to_proto(&input_json)?;

            let req = ProveVeraTaskRequest {
                app_id: cmd.app_id.clone(),
                task_id: cmd.task_id.clone(),
                input: Some(vera_input),
            };
            let res = client.prove_vera_task(req).await?.into_inner();

            info!("ProveVera: err={:?}", res.err);

            // Poll for the proving result
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

                let req = GetProvingResultRequest {
                    app_id: cmd.app_id.clone(),
                    task_id: cmd.task_id.clone(),
                };
                let res = client.get_proving_result(req).await?.into_inner();

                if let (Some(proof), Some(guest_output)) = (res.proof, res.guest_output) {
                    // Save proof
                    let output_path = cmd
                        .output
                        .unwrap_or_else(|| PathBuf::from(format!("proof_{}.bin", cmd.task_id)));
                    fs::write(&output_path, &proof)?;
                    info!(
                        "ProveVera: proof saved to {:?} ({} bytes)",
                        output_path,
                        proof.len()
                    );

                    // Save guest_output as JSON
                    let guest_output_path = cmd.guest_output.unwrap_or_else(|| {
                        PathBuf::from(format!("guest_output_{}.json", cmd.task_id))
                    });
                    let guest_output_json = serialize_guest_output_to_json(&guest_output);
                    fs::write(&guest_output_path, &guest_output_json)?;
                    info!("ProveVera: guest_output saved to {:?}", guest_output_path);

                    break;
                } else if let Some(ref err) = res.err {
                    if err.code != 0 {
                        info!("ProveVera: proving failed: {:?}", err);
                        break;
                    }
                }
                info!("ProveVera: still proving...");
            }
        }
        Command::VerifyVera(cmd) => {
            let proof = fs::read(&cmd.proof)?;
            let output_json = fs::read_to_string(&cmd.output)?;
            let vera_output = parse_guest_output_to_proto(&output_json)?;

            let req = VerifyVeraProofRequest {
                app_id: cmd.app_id,
                proof,
                output: Some(vera_output),
            };
            let res = client.verify_vera_proof(req).await?.into_inner();

            info!("VerifyVera: err={:?}, verified={}", res.err, res.verified);
        }
    }

    Ok(())
}

// ─── Helper functions for parsing GuestInput/GuestOutput JSON ───────────────────

fn parse_guest_input_to_proto(json: &str) -> Result<VeraInput> {
    // Parse the JSON manually to handle Rust Debug format for transforms
    let value: serde_json::Value = serde_json::from_str(json)?;

    let jpeg_pre_manifest = hex::decode(value["jpeg_pre_manifest"].as_str().unwrap())?;
    let jpeg_post_manifest = hex::decode(value["jpeg_post_manifest"].as_str().unwrap())?;
    let c2pa_protected_headers = hex::decode(value["c2pa_protected_headers"].as_str().unwrap())?;
    let c2pa_claim_cbor = hex::decode(value["c2pa_claim_cbor"].as_str().unwrap())?;
    let c2pa_signature_r = hex::decode(value["c2pa_signature_r"].as_str().unwrap())?;
    let c2pa_signature_s = hex::decode(value["c2pa_signature_s"].as_str().unwrap())?;
    let c2pa_pubkey = hex::decode(value["c2pa_pubkey"].as_str().unwrap())?;
    let c2pa_data_hash_assertion_raw =
        hex::decode(value["c2pa_data_hash_assertion_raw"].as_str().unwrap())?;

    // Parse transforms - new JSON format: [{"Crop":{"x":0,"y":0,"width":160,"height":106}}, {"Brighten":{"value":10}}]
    let transforms_array = value["transforms"].as_array().unwrap();
    let transforms: Vec<Transform> = transforms_array
        .iter()
        .map(|t| {
            let obj = t.as_object().unwrap();
            if let Some(crop) = obj.get("Crop") {
                let inner = crop.as_object().unwrap();
                Transform {
                    transform: Some(pico_proving_service::transform::Transform::Crop(
                        pico_proving_service::CropTransform {
                            x: inner["x"].as_u64().unwrap() as u32,
                            y: inner["y"].as_u64().unwrap() as u32,
                            width: inner["width"].as_u64().unwrap() as u32,
                            height: inner["height"].as_u64().unwrap() as u32,
                        },
                    )),
                }
            } else if let Some(brighten) = obj.get("Brighten") {
                let inner = brighten.as_object().unwrap();
                Transform {
                    transform: Some(pico_proving_service::transform::Transform::Brighten(
                        pico_proving_service::BrightenTransform {
                            value: inner["value"].as_i64().unwrap() as i32,
                        },
                    )),
                }
            } else if obj.contains_key("Grayscale") {
                Transform {
                    transform: Some(pico_proving_service::transform::Transform::Grayscale(
                        pico_proving_service::GrayscaleTransform {},
                    )),
                }
            } else {
                panic!("Unknown transform type");
            }
        })
        .collect();

    // Parse exclusions (now properly formatted as empty array [])
    let exclusions_array = value["c2pa_pre_hash_exclusions"].as_array().unwrap();
    let c2pa_pre_hash_exclusions: Vec<_> = exclusions_array
        .iter()
        .map(|e| {
            let inner = e.as_array().unwrap();
            pico_proving_service::C2paPreHashExclusion {
                start_offset: inner[0].as_u64().unwrap() as u32,
                length: inner[1].as_u64().unwrap() as u32,
            }
        })
        .collect();

    Ok(VeraInput {
        jpeg_pre_manifest,
        jpeg_post_manifest,
        transforms,
        c2pa_protected_headers,
        c2pa_claim_cbor,
        c2pa_sig_r: c2pa_signature_r,
        c2pa_sig_s: c2pa_signature_s,
        c2pa_pubkey,
        c2pa_data_hash_assertion_raw,
        c2pa_pre_hash_exclusions,
    })
}

fn parse_guest_output_to_proto(json: &str) -> Result<VeraOutput> {
    // Parse the JSON manually
    let value: serde_json::Value = serde_json::from_str(json)?;

    let original_hash = hex::decode(value["original_hash"].as_str().unwrap())?;
    let output_hash = hex::decode(value["output_hash"].as_str().unwrap())?;
    let c2pa_pubkey = hex::decode(value["c2pa_pubkey"].as_str().unwrap())?;

    // Parse transforms_applied - they are in format ["Crop", "Brighten"]
    let transforms_array = value["transforms_applied"].as_array().unwrap();
    let transforms_applied: Vec<i32> = transforms_array
        .iter()
        .map(|t| {
            let s = t.as_str().unwrap();
            match s {
                "Crop" => 0,
                "Brighten" => 1,
                "Grayscale" => 2,
                _ => panic!("Unknown transform kind: {}", s),
            }
        })
        .collect();

    Ok(VeraOutput {
        original_hash,
        output_hash,
        transforms_applied,
        c2pa_pubkey,
    })
}

/// Serialize VeraOutput to JSON with hex strings.
fn serialize_guest_output_to_json(output: &VeraOutput) -> String {
    let mut json = String::from("{\n");

    // original_hash as hex
    json.push_str(&format!(
        "  \"original_hash\": \"{}\",\n",
        hex::encode(&output.original_hash)
    ));

    // output_hash as hex
    json.push_str(&format!(
        "  \"output_hash\": \"{}\",\n",
        hex::encode(&output.output_hash)
    ));

    // transforms_applied as string array
    json.push_str("  \"transforms_applied\": [\n");
    for (i, t) in output.transforms_applied.iter().enumerate() {
        if i > 0 {
            json.push_str(",\n");
        }
        let name = match t {
            0 => "Crop",
            1 => "Brighten",
            2 => "Grayscale",
            _ => "Unknown",
        };
        json.push_str(&format!("    \"{}\"", name));
    }
    json.push_str("\n  ],\n");

    // c2pa_pubkey as hex
    json.push_str(&format!(
        "  \"c2pa_pubkey\": \"{}\"\n",
        hex::encode(&output.c2pa_pubkey)
    ));

    json.push_str("}\n");
    json
}
