use crate::{
    proving::worker::prover::{Prover, ProverRunner},
    proving_queue::ProvingTask,
};
use anyhow::Result;
use futures::future::join_all;
use pico_vm::thread::channel::{DuplexUnboundedChannel, SingleUnboundedChannel};
use tracing::info;

mod emulator;
pub mod gateway;
pub mod messages;
pub mod onchain;
pub mod worker;

pub async fn prove_task(task: ProvingTask, prover_count: usize) -> Result<Vec<u8>> {
    info!("[proving] starting prove_task for: {:?}", task.key);

    // Create a completion signal with proof result
    let (completion_sender, completion_receiver) = tokio::sync::oneshot::channel();

    let emulator_gateway_channel = SingleUnboundedChannel::default();
    let gateway_worker_channel = DuplexUnboundedChannel::default();

    // start gateway with proof callback
    let gateway_handle = gateway::run(
        emulator_gateway_channel.receiver(),
        gateway_worker_channel.endpoint1(),
        completion_sender,
    );

    // start provers
    let provers: Vec<_> = (0..prover_count)
        .enumerate()
        .map(|(i, _)| {
            let prover_id = format!("prover-{i}");
            let worker_endpoint = gateway_worker_channel.endpoint2().clone_inner();

            if task.use_gpu {
                info!("[proving] creating CUDA prover: {}", prover_id);
                let prover = Prover::new_cuda(prover_id, worker_endpoint, task.clone());
                prover.run_cuda()
            } else {
                info!("[proving] creating CPU prover: {}", prover_id);
                let prover = Prover::new(prover_id, worker_endpoint, task.clone());
                prover.run()
            }
        })
        .collect();

    // start emulator
    // We no longer need an emulator channel and sending start message
    emulator::run(task, emulator_gateway_channel.sender());

    // Wait for proving to complete
    info!("[proving] waiting for proving to complete");

    // Wait for completion signal from gateway and get the proof
    let proof_bytes = completion_receiver.await?;
    info!("[proving] received completion signal from gateway with proof");

    // Wait for all handles to complete (with timeout to avoid hanging)
    let timeout = tokio::time::timeout(std::time::Duration::from_secs(5), join_all(provers)).await;
    match timeout {
        Ok(_) => info!("[proving] all provers completed"),
        Err(_) => info!("[proving] provers completion timed out"),
    }

    // Wait for gateway to complete (with timeout)
    let timeout = tokio::time::timeout(std::time::Duration::from_secs(5), gateway_handle).await;
    match timeout {
        Ok(_) => info!("[proving] gateway completed"),
        Err(_) => info!("[proving] gateway completion timed out"),
    }

    info!("[proving] proving workflow completed successfully");
    Ok(proof_bytes)
}
