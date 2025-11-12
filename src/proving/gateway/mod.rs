use crate::proving::{
    messages::{combine::CombineMsg, gateway::GatewayMsg, riscv::RiscvMsg},
    onchain::prove_embed_onchain,
};
use crossbeam::channel::{Receiver, select_biased};
use handler::GatewayHandler;
use log::debug;
use pico_vm::thread::channel::DuplexUnboundedEndpoint;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{error, info};

pub(crate) mod handler;

pub type GatewayEndpoint = DuplexUnboundedEndpoint<GatewayMsg, GatewayMsg>;

pub fn run(
    // exit the whole app directly if proving complete
    emulator_receiver: Arc<Receiver<GatewayMsg>>,
    grpc_endpoint: Arc<GatewayEndpoint>,
    completion_sender: tokio::sync::oneshot::Sender<Vec<u8>>,
) -> JoinHandle<()> {
    debug!("[coordinator] gateway init with proof callback");

    let thread_handle = tokio::task::spawn_blocking(move || {
        let mut gateway_handler: GatewayHandler = GatewayHandler::new();
        let mut completion_sender = Some(completion_sender);

        loop {
            select_biased! {
                recv(emulator_receiver) -> msg => {
                    let msg = match msg {
                        Ok(msg) => msg,
                        Err(_) => break, // Channel closed, exit gracefully
                    };
                    match msg {
                        GatewayMsg::Riscv(RiscvMsg::Request(..), _, _) => {
                            let no_task = gateway_handler.process_riscv_req(&msg).unwrap();
                            assert!(no_task.is_none());
                            // send the task to grpc
                            grpc_endpoint.send(msg).unwrap();
                        }
                        GatewayMsg::EmulatorComplete => {
                            if let Some(exit_msg) = gateway_handler.process(msg.clone()).unwrap() {
                                if matches!(exit_msg, GatewayMsg::Exit) {
                                    break; // Exit the gateway loop
                                }
                            }
                        }
                        _ => panic!("unsupported"),
                    }
                }
                recv(grpc_endpoint.receiver()) -> msg => {
                    let msg = match msg {
                        Ok(msg) => msg,
                        Err(_) => break, // Channel closed, exit gracefully
                    };
                    match msg {
                        GatewayMsg::Riscv(RiscvMsg::Response(..), _, _)
                        | GatewayMsg::Combine(CombineMsg::Response(..), _, _)
                        | GatewayMsg::Embed(..)
                        | GatewayMsg::Exit => {
                            // save the generated proof to the chunk_index slot in proof tree
                            if let Some(msg) = gateway_handler.process(msg.clone()).unwrap() {
                                match msg {
                                    GatewayMsg::Exit => {
                                        info!("[gateway] received Exit message, proving complete");
                                        // Proving is complete. Generate on-chain proof and send via callback
                                        if let Some(embed_proof) = gateway_handler.get_embed_proof() {
                                            // Run on-chain dockerized phase to obtain final proof bytes
                                            let proof_bytes = match prove_embed_onchain(embed_proof) {
                                                Ok(bytes) => bytes,
                                                Err(e) => {
                                                    error!("[gateway] on-chain proof generation failed: {}", e);
                                                    vec![]
                                                }
                                            };
                                            let proof_size = proof_bytes.len();
                                            info!("[gateway] sending final on-chain proof via callback, size: {} bytes", proof_size);

                                            // Send proof via completion signal
                                            if let Some(sender) = completion_sender.take() {
                                                let _ = sender.send(proof_bytes);
                                            }
                                        } else {
                                            error!("[gateway] Exit received but no embed proof available");
                                            // Send empty proof to avoid hanging
                                            if let Some(sender) = completion_sender.take() {
                                                let _ = sender.send(vec![]);
                                            }
                                        }
                                        break; // Exit the gateway loop
                                    }
                                    _ => {
                                        // send the new task (combine, compress, or embed) to grpc
                                        grpc_endpoint.send(msg).unwrap();
                                    }
                                }
                            }
                        }
                        // nothing to do here, this's used for single-node
                        GatewayMsg::RequestTask => (),
                        _ => panic!("unsupported"),
                    }
                }
            }
        }
    });

    debug!("[coordinator] gateway init end");

    thread_handle
}

// on-chain proof generation handled in Exit branch above
