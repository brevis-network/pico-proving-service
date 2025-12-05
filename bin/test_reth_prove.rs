use alloy_chains::Chain;
use alloy_provider::{Provider, ProviderBuilder, RootProvider, WsConnect, network::AnyNetwork};
use anyhow::{Result, anyhow};
use clap::Parser;
use dotenvy::dotenv;
use futures::{StreamExt, future::ready};
use pico_proving_service::{
    EstimateCostRequest, ProveTaskRequest, RegisterAppRequest, app_manager::App,
    prover_network_client::ProverNetworkClient,
};
use pico_vm::{
    configs::stark_config::KoalaBearPoseidon2 as SC, emulator::stdin::EmulatorStdinBuilder,
    machine::logger::setup_logger,
};
use rsp_client_executor::io::EthClientExecutorInput;
use rsp_host_executor::{
    BlockExecutor, Config as BlockExecutorConfig, EthExecutorComponents, FullExecutor,
    create_eth_block_execution_strategy_factory,
};
use rsp_provider::create_provider;
use std::{fs, path::PathBuf};
use tonic::{codec::CompressionEncoding, transport::Channel};
use tracing::{info, warn};
use url::Url;

// reth elf file path
const RETH_ELF_PATH: &str = "fixtures/reth-elf";

#[derive(Parser)]
struct Cli {
    #[clap(long, env = "PICO_RPC_URL", help = "HTTP RPC URL")]
    rpc_http_url: Url,

    #[clap(long, env = "PICO_WS_RPC_URL", help = "WebSocket RPC URL")]
    rpc_ws_url: Url,

    #[clap(
        long,
        default_value_t = 100,
        help = "Interval at which to execute blocks"
    )]
    block_interval: u64,

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

    #[clap(long, default_value = "cache_dir", help = "Input cache directory")]
    cache_dir: PathBuf,

    #[arg(
        long,
        help = "Whether to use GPU for proving (default: false, use CPU)"
    )]
    use_gpu: bool,

    #[arg(long, help = "Whether to estimate cost (default: false)")]
    estimate_cost: bool,
}

impl Cli {
    // parse the block executor configuration
    async fn block_executor_config(&self) -> Result<BlockExecutorConfig> {
        // get the chain ID
        let provider = RootProvider::<AnyNetwork>::new_http(self.rpc_http_url.clone());
        let chain_id = provider.get_chain_id().await?;

        // build chain and genesis
        let chain = Chain::from_id(chain_id);
        let genesis = chain_id.try_into()?;

        Ok(BlockExecutorConfig {
            chain,
            genesis,
            rpc_url: Some(self.rpc_http_url.clone()),
            cache_dir: Some(self.cache_dir.clone()),
            custom_beneficiary: None,
            prove_mode: None,
            skip_client_execution: false,
            opcode_tracking: false,
        })
    }
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

    info!("registering reth application");
    let app = register_reth(&mut prover_network_client).await?;
    let app_id = app.app_id;

    info!("initializing block executor");
    let block_executor = block_executor(&cli).await?;

    info!("initializing WebSocket RPC connection for receiving latest blocks");
    let ws_conn = WsConnect::new(cli.rpc_ws_url);
    let ws_provider = ProviderBuilder::new().connect_ws(ws_conn).await?;
    let subscription = ws_provider.subscribe_blocks().await?;
    let mut latest_block_receiver = subscription
        .into_stream()
        .filter(|header| ready(header.number % cli.block_interval == 0));

    info!("start to emulate and prove latest blocks");
    while let Some(header) = latest_block_receiver.next().await {
        let block_number = header.number;
        info!("waiting for block-{block_number}");
        block_executor
            .wait_for_block(block_number)
            .await
            .map_err(|e| anyhow!("failed to wait for block-{block_number}: {e:?}"))?;

        info!("fetching block-{block_number}");
        let client_input = block_executor
            .execute(header.number, None)
            .await
            .map_err(|e| anyhow!("failed to fetch block-{block_number}: {e:?}"))?;

        info!("generating block-{block_number} input");
        let mut stdin_builder = EmulatorStdinBuilder::<Vec<u8>, SC>::default();
        stdin_builder.write::<EthClientExecutorInput>(&client_input);
        let block_inputs = bincode::serialize(&stdin_builder)?;

        info!("sending ProveTask request to service");
        prove_task(
            &mut prover_network_client,
            ProveTaskRequest {
                app_id: app_id.clone(),
                task_id: format!("task-block-{block_number}"),
                inputs: Some(block_inputs.clone()),
                use_gpu: Some(cli.use_gpu),
            },
        )
        .await?;

        if cli.estimate_cost {
            info!("sending EstimateCost request to service");
            estimate_cost(
                &mut prover_network_client,
                EstimateCostRequest {
                    app_id: app_id.clone(),
                    inputs: Some(block_inputs),
                },
            )
            .await?;
        }
    }

    Ok(())
}

// initialize a block executor
async fn block_executor(
    cli: &Cli,
) -> Result<FullExecutor<EthExecutorComponents<()>, RootProvider>> {
    let rpc_http_provider = create_provider(cli.rpc_http_url.clone());
    let current_block_number = rpc_http_provider.get_block_number().await?;
    info!("current latest block number is {current_block_number}");

    let config = cli.block_executor_config().await?;
    let block_execution_strategy_factory =
        create_eth_block_execution_strategy_factory(&config.genesis, config.custom_beneficiary);
    FullExecutor::<EthExecutorComponents<_>, _>::try_new(
        rpc_http_provider,
        block_execution_strategy_factory,
        (),
        config,
    )
    .await
    .map_err(|e| anyhow!("failed to initialize block executor: {e:?}"))
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

// register the reth app
async fn register_reth(prover_network_client: &mut ProverNetworkClient<Channel>) -> Result<App> {
    let elf = fs::read(RETH_ELF_PATH)?;

    // generate app id
    let app = App::new(&elf, None);

    // register reth app to service
    let req = RegisterAppRequest { elf, info: None };
    if let Err(e) = prover_network_client.register_app(req).await {
        // ouput and ignore the error since it may have always been registered
        warn!("RegisterApp: err={e:?}");
    }

    Ok(app)
}

// estimate cost for a specified block
async fn estimate_cost(
    prover_network_client: &mut ProverNetworkClient<Channel>,
    request: EstimateCostRequest,
) -> Result<()> {
    let res = prover_network_client
        .estimate_cost(request)
        .await?
        .into_inner();

    info!(
        "EstimateCost: err={:?}, cost={}, pv_digest={:?}",
        res.err, res.cost, res.pv_digest,
    );
    Ok(())
}

// send a proving task for a specified block
async fn prove_task(
    prover_network_client: &mut ProverNetworkClient<Channel>,
    request: ProveTaskRequest,
) -> Result<()> {
    let res = prover_network_client
        .prove_task(request)
        .await?
        .into_inner();

    info!("ProveTask: err={:?}", res.err);

    Ok(())
}
