use anyhow::{Result, bail};
use clap::Parser;
use dotenvy::dotenv;
use itertools::Itertools;
use pico_proving_service::{
    EstimateCostRequest, EstimateCostResponse, GetProvingResultRequest, ProveAggTaskRequest,
    ProveTaskRequest, ProvingStage, RegisterAppRequest, SubAppInput, app_manager::App,
    prover_network_client::ProverNetworkClient,
};
use pico_vm::{
    configs::stark_config::KoalaBearPoseidon2 as SC, emulator::stdin::EmulatorStdinBuilder,
    machine::logger::setup_logger,
};
use rand::{Rng, thread_rng};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::time::{Duration, sleep};
use tonic::{codec::CompressionEncoding, transport::Channel};
use tracing::{info, warn};

// app input data
const FIBONACCI_APP_INPUT: u32 = 10;
const KECCAK_APP_INPUT: &[u8] = &[1, 2, 3, 4];
const SHA2_APP_INPUT: &[u8] = &[5, 6, 7, 8];

// app elf file paths
const AGG_APP_ELF_PATH: &str = "apps/aggregator/elf/riscv32im-pico-zkvm-elf";
const FIBONACCI_APP_ELF_PATH: &str = "apps/fibonacci/elf/riscv32im-pico-zkvm-elf";
const KECCAK_APP_ELF_PATH: &str = "apps/keccak/elf/riscv32im-pico-zkvm-elf";
const SHA2_APP_ELF_PATH: &str = "apps/sha2/elf/riscv32im-pico-zkvm-elf";

// interval seconds for getting the proving result
const GET_PROVING_RESULT_INTERVAL_SECS: u64 = 10;

// maximum times for getting the proving result
const GET_PROVING_RESULT_MAX_TIMES: u32 = 30;

#[derive(Parser)]
struct Cli {
    #[clap(
        long,
        env = "GRPC_ADDR",
        default_value = "http://[::]:50052",
        help = "gRPC address"
    )]
    grpc_addr: String,

    #[clap(
        long,
        env = "MAX_GRPC_MSG_SIZE",
        default_value = "1073741824",
        help = "Max gRPC message size (bytes)"
    )]
    max_grpc_msg_size: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    info!("initializing ENV and setup logger");
    dotenv().ok();
    setup_logger();

    info!("parsing CLI arguments");
    let cli = Cli::parse();

    info!("initializing prover network client");
    let mut prover_network_client = prover_network_client(&cli).await?;

    info!("registering sub and aggregate applications");
    let sub_app1 = register_app(&mut prover_network_client, FIBONACCI_APP_ELF_PATH).await?;
    let sub_app2 = register_app(&mut prover_network_client, SHA2_APP_ELF_PATH).await?;
    let sub_app3 = register_app(&mut prover_network_client, KECCAK_APP_ELF_PATH).await?;
    let sub_apps = vec![&sub_app1, &sub_app2, &sub_app3];
    let agg_app = register_app(&mut prover_network_client, AGG_APP_ELF_PATH).await?;

    info!("getting sub application vk digests");
    let sub_vk_digest1 = sub_app1.vk_digest();
    let sub_vk_digest2 = sub_app2.vk_digest();
    let sub_vk_digest3 = sub_app3.vk_digest();
    let agg_vk_digest = agg_app.vk_digest();

    info!("constructing sub application inputs");
    let sub_input1 = u32_input(FIBONACCI_APP_INPUT)?;
    let sub_input2 = bytes_input(SHA2_APP_INPUT)?;
    let sub_input3 = bytes_input(KECCAK_APP_INPUT)?;
    let sub_inputs = vec![&sub_input1, &sub_input2, &sub_input3];

    info!("getting sub application public value digests by calling estimate-cost requests");
    let mut sub_pv_digests = vec![];
    for (app, input) in sub_apps.iter().zip_eq(sub_inputs.iter()) {
        let pv_digest: [u8; 32] = estimate_cost(
            &mut prover_network_client,
            EstimateCostRequest {
                app_id: app.app_id.clone(),
                inputs: Some(input.to_vec()),
            },
        )
        .await?
        .raw_pv_digest
        .try_into()
        .expect("public value digest must be [u8; 32]");

        sub_pv_digests.push(pv_digest);
    }
    let sub_pv_digest1 = sub_pv_digests[0];
    let sub_pv_digest2 = sub_pv_digests[1];
    let sub_pv_digest3 = sub_pv_digests[2];

    info!("proving sub applications to generate proofs");
    let mut sub_proofs = vec![];
    for (app, input) in sub_apps.iter().zip_eq(sub_inputs.iter()) {
        let req = ProveTaskRequest {
            app_id: app.app_id.clone(),
            task_id: random_task_id(),
            inputs: Some(input.to_vec()),
            use_gpu: None,
        };
        let proof = prove_sub_task(&mut prover_network_client, req).await?;

        sub_proofs.push(proof);
    }
    let [sub_proof1, sub_proof2, sub_proof3] = sub_proofs.try_into().unwrap();

    info!("proving aggregator for the first two sub applications");
    let (agg_pv_digest, agg_proof) = prove_agg_task(
        &mut prover_network_client,
        ProvingStage::Intermediate,
        agg_app.app_id.clone(),
        vec![sub_app1.app_id, sub_app2.app_id, sub_app3.app_id],
        vec![sub_pv_digest1, sub_pv_digest2, sub_pv_digest3],
        vec![sub_vk_digest1, sub_vk_digest2, sub_vk_digest3],
        vec![sub_proof1, sub_proof2, sub_proof3],
    )
    .await?;

    // TODO: fix to support nested deferred proofs
    // info!("proving aggregator for the third sub and the aggregator applications");
    // let _ = prove_agg_task(
    //     &mut prover_network_client,
    //     ProvingStage::Final,
    //     agg_app.app_id.clone(),
    //     vec![sub_app3.app_id, agg_app.app_id],
    //     vec![sub_pv_digest3, agg_pv_digest],
    //     vec![sub_vk_digest3, agg_vk_digest],
    //     vec![sub_proof3, agg_proof],
    // )
    // .await?;

    Ok(())
}

// get a random task ID
fn random_task_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let rand_val: u32 = thread_rng().gen_range(0..=u32::MAX);

    format!("task-{now}-{rand_val:x}")
}

// construct an uint32 app input
fn u32_input(n: u32) -> Result<Vec<u8>> {
    let mut stdin_builder = EmulatorStdinBuilder::<Vec<u8>, SC>::default();
    stdin_builder.write::<u32>(&n);
    let input = bincode::serialize(&stdin_builder)?;

    Ok(input)
}

// construct a bytes app input
fn bytes_input(data: &[u8]) -> Result<Vec<u8>> {
    let mut stdin_builder = EmulatorStdinBuilder::<Vec<u8>, SC>::default();
    stdin_builder.write::<Vec<u8>>(&data.to_vec());
    let input = bincode::serialize(&stdin_builder)?;

    Ok(input)
}

// construct an aggregator app input
fn aggregator_input(vk_digests: Vec<[u32; 8]>, pv_digests: Vec<[u8; 32]>) -> Result<Vec<u8>> {
    let mut stdin_builder = EmulatorStdinBuilder::<Vec<u8>, SC>::default();
    stdin_builder.write::<Vec<[u32; 8]>>(&vk_digests);
    stdin_builder.write::<Vec<[u8; 32]>>(&pv_digests);
    let input = bincode::serialize(&stdin_builder)?;

    Ok(input)
}

// initialize a prover network client
async fn prover_network_client(cli: &Cli) -> Result<ProverNetworkClient<Channel>> {
    let prover_network_client = ProverNetworkClient::connect(cli.grpc_addr.clone())
        .await?
        .max_encoding_message_size(cli.max_grpc_msg_size)
        .max_decoding_message_size(cli.max_grpc_msg_size)
        .accept_compressed(CompressionEncoding::Zstd)
        .send_compressed(CompressionEncoding::Zstd);

    Ok(prover_network_client)
}

// register an elf application
async fn register_app(
    prover_network_client: &mut ProverNetworkClient<Channel>,
    elf_file_path: &str,
) -> Result<App> {
    let elf = fs::read(elf_file_path)?;

    // generate the app id
    let app = App::new(&elf, None);

    // register the app to service
    let req = RegisterAppRequest { elf, info: None };
    match prover_network_client.register_app(req).await {
        Ok(res) => {
            let res = res.into_inner();
            assert_eq!(res.raw_vk_digest, app.vk_digest(), "return wrong vk digest");
        }
        Err(e) => {
            // ouput and ignore the error since it may have always been registered
            warn!("RegisterApp: {e:?}");
        }
    }

    Ok(app)
}

// estimate cost for a request
async fn estimate_cost(
    prover_network_client: &mut ProverNetworkClient<Channel>,
    request: EstimateCostRequest,
) -> Result<EstimateCostResponse> {
    let res = prover_network_client
        .estimate_cost(request)
        .await?
        .into_inner();

    if let Some(e) = res.err {
        panic!("EstimateCost: {e:?}");
    }

    Ok(res)
}

// generate an intermediate proof for a sub-task request
async fn prove_sub_task(
    prover_network_client: &mut ProverNetworkClient<Channel>,
    request: ProveTaskRequest,
) -> Result<Vec<u8>> {
    let app_id = request.app_id.clone();
    let task_id = request.task_id.clone();

    let res = prover_network_client
        .prove_sub_task(request)
        .await?
        .into_inner();
    if let Some(e) = res.err {
        panic!("ProveSubTask: {e:?}");
    }

    let proof = wait_for_proving_result(
        prover_network_client,
        GetProvingResultRequest { app_id, task_id },
    )
    .await?;

    Ok(proof)
}

// generate an aggregate proof; return the aggregate public values digest and proof
async fn prove_agg_task(
    prover_network_client: &mut ProverNetworkClient<Channel>,
    proving_stage: ProvingStage,
    agg_app_id: String,
    sub_app_ids: Vec<String>,
    sub_pv_digests: Vec<[u8; 32]>,
    sub_vk_digests: Vec<[u32; 8]>,
    sub_proofs: Vec<Vec<u8>>,
) -> Result<([u8; 32], Vec<u8>)> {
    info!("constructing aggregate input");
    let agg_input = aggregator_input(sub_vk_digests, sub_pv_digests.clone())?;

    info!("getting aggregator application public value digest by calling estimate-cost request");
    let pv_digest: [u8; 32] = estimate_cost(
        prover_network_client,
        EstimateCostRequest {
            app_id: agg_app_id.clone(),
            inputs: Some(agg_input.clone()),
        },
    )
    .await?
    .raw_pv_digest
    .try_into()
    .expect("public value digest must be [u8; 32]");

    info!("constructing aggregate proving task request");
    let sub_app_inputs = sub_app_ids
        .into_iter()
        .zip_eq(sub_pv_digests)
        .zip_eq(sub_proofs)
        .map(|((app_id, pv_digest), proof)| SubAppInput {
            app_id,
            proof,
            raw_pv_digest: pv_digest.to_vec(),
        })
        .collect();
    let task_id = random_task_id();
    let req = ProveAggTaskRequest {
        app_id: agg_app_id.clone(),
        task_id: task_id.clone(),
        inputs: Some(agg_input),
        sub_app_inputs,
        stage: proving_stage.into(),
    };

    info!("generating aggregate proof");
    let res = prover_network_client
        .prove_agg_task(req)
        .await?
        .into_inner();

    if let Some(e) = res.err {
        panic!("ProveAggTask: {e:?}");
    }

    let proof = wait_for_proving_result(
        prover_network_client,
        GetProvingResultRequest {
            app_id: agg_app_id,
            task_id,
        },
    )
    .await?;

    Ok((pv_digest, proof))
}

// wait for a proving task complete and return the proof
async fn wait_for_proving_result(
    prover_network_client: &mut ProverNetworkClient<Channel>,
    request: GetProvingResultRequest,
) -> Result<Vec<u8>> {
    for i in 0..GET_PROVING_RESULT_MAX_TIMES {
        let res = prover_network_client
            .get_proving_result(request.clone())
            .await?
            .into_inner();

        if let Some(proof) = res.proof {
            return Ok(proof);
        }

        info!("no proof return from GetProvingResult in attempt-{i}");
        sleep(Duration::from_secs(GET_PROVING_RESULT_INTERVAL_SECS)).await;
    }

    bail!("failed to request GetProvingResult after {GET_PROVING_RESULT_MAX_TIMES} attempts")
}
