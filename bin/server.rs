use anyhow::Result;
use clap::Parser;
use dotenvy::dotenv;
use pico_proving_service::{
    config::ServiceConfig,
    grpc::GrpcService,
    proving::onchain::start_onchain_daemon,
    proving_queue::{ProvingOutputs, ProvingQueue},
};
use pico_vm::{
    iter::{ThreadPoolBuilder, current_num_threads},
    machine::logger::setup_logger,
    thread::channel::SingleUnboundedChannel,
};
use sqlx::sqlite::SqlitePoolOptions;
use std::{process::exit, sync::Arc};
use tokio::signal::ctrl_c;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    setup_logger();
    // Start background docker monitor for on-chain prover
    start_onchain_daemon();

    ThreadPoolBuilder::new()
        .num_threads(num_cpus::get())
        .build_global()
        .expect("failed to build global Rayon thread pool");
    info!("initialized Rayon with {} threads", current_num_threads());

    let cfg = ServiceConfig::parse();
    info!("starting with config: {:?}", cfg);

    let db_pool = Arc::new(SqlitePoolOptions::new().connect(&cfg.db_url).await?);
    let proving_outputs = Arc::new(ProvingOutputs::default());
    let grpc_to_proving_channel = SingleUnboundedChannel::default();

    let mut handles = vec![];

    let proving_queue = ProvingQueue::new(
        cfg.clone(),
        proving_outputs.clone(),
        grpc_to_proving_channel.receiver(),
        db_pool.clone(),
    );
    handles.push(proving_queue.run());

    let grpc_service = GrpcService::new(
        cfg,
        db_pool,
        proving_outputs,
        grpc_to_proving_channel.sender(),
    );
    handles.push(grpc_service.run());

    info!("waiting for stop");
    ctrl_c().await?;

    info!("server exits");
    exit(0);
}
