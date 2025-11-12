pub mod combine;
pub mod compress;
pub mod embed;
pub mod riscv_convert;

use super::WorkerEndpoint;
use crate::{
    proving::messages::{
        combine::CombineMsg, embed::EmbedRequest, gateway::GatewayMsg, riscv::RiscvMsg,
    },
    proving_queue::ProvingTask,
    types::{SC, Val},
};
use combine::{CombineHandler, CombineProver};
use compress::{CompressHandler, CompressProver};
use embed::{EmbedHandler, EmbedProver};
use p3_field::FieldAlgebra;
use pico_vm::{
    instances::compiler::vk_merkle::{HasStaticVkManager, VkMerkleManager},
    primitives::consts::DIGEST_SIZE,
};
use riscv_convert::{RiscvConvertHandler, RiscvConvertProver};
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{error, info};

type VkRoot = [Val; DIGEST_SIZE];

pub struct Prover {
    prover_id: String,
    endpoint: Arc<WorkerEndpoint>,
    riscv_convert: RiscvConvertProver,
    combine: CombineProver,
    compress: CompressProver,
    embed: EmbedProver,
    vk_root: VkRoot,
}

impl Prover {
    pub fn new(prover_id: String, endpoint: Arc<WorkerEndpoint>, task: ProvingTask) -> Self {
        let riscv_convert = RiscvConvertProver::new(prover_id.clone(), task);
        let combine = CombineProver::new(prover_id.clone());
        let compress = CompressProver::new(prover_id.clone());
        let embed = EmbedProver::new(prover_id.clone());

        let vk_manager = <SC as HasStaticVkManager>::static_vk_manager();
        let vk_root = get_vk_root(vk_manager);

        Self {
            prover_id,
            endpoint,
            riscv_convert,
            combine,
            compress,
            embed,
            vk_root,
        }
    }

    /// Create a new CUDA GPU prover (unimplemented for now)
    pub fn new_cuda(
        _prover_id: String,
        _endpoint: Arc<WorkerEndpoint>,
        _task: ProvingTask,
    ) -> Self {
        unimplemented!()
    }
}

/// specialization for running emulator on either babybear or koalabear
pub trait ProverRunner {
    fn run(self) -> JoinHandle<()>;
    fn run_cuda(self) -> JoinHandle<()>;
}

impl ProverRunner for Prover {
    fn run(self) -> JoinHandle<()> {
        info!("[{}] : start", self.prover_id);

        tokio::task::spawn_blocking(move || {
            // request for task first
            let msg = GatewayMsg::RequestTask;
            self.endpoint.send(msg).unwrap();

            while let Ok(msg) = self.endpoint.recv() {
                match msg {
                    GatewayMsg::Riscv(RiscvMsg::Request(req), task_id, ip_addr) => {
                        info!(
                            "[{}] receive riscv request of chunk-{}",
                            self.prover_id, &req.chunk_index,
                        );
                        let res = self.riscv_convert.process(req, &self.vk_root);
                        info!(
                            "[{}] send riscv response of chunk-{}",
                            self.prover_id, &res.chunk_index,
                        );
                        let msg = GatewayMsg::Riscv(RiscvMsg::Response(res), task_id, ip_addr);
                        self.endpoint.send(msg).unwrap();
                    }
                    GatewayMsg::Combine(CombineMsg::Request(req), task_id, ip_addr) => {
                        info!(
                            "[{}] receive combine request of chunk-{}",
                            self.prover_id, &req.chunk_index,
                        );
                        let flag_complete = req.flag_complete;
                        let res = self.combine.process(req);
                        if flag_complete {
                            // Direct execution of compress and embed phases
                            info!(
                                "[{}] final combine complete, executing compress phase directly",
                                self.prover_id
                            );
                            let compress_res = self.compress.process(compress::CompressRequest {
                                chunk_index: res.chunk_index,
                                proof: res.proof.clone(),
                            });

                            info!(
                                "[{}] compress complete, executing embed phase directly",
                                self.prover_id
                            );
                            let embed_res = self.embed.process(EmbedRequest {
                                chunk_index: compress_res.chunk_index,
                                proof: compress_res.proof,
                            });

                            // Verify the final embed proof before sending
                            if self
                                .embed
                                .verify(&embed_res.proof.inner, self.riscv_convert.riscv_vk())
                                .is_ok()
                            {
                                info!("[{}] succeeded to verify final embed proof", self.prover_id,);

                                // Send the embed proof directly to gateway
                                info!(
                                    "[{}] embed complete, sending embed proof to gateway",
                                    self.prover_id
                                );
                                let embed_proof_msg =
                                    GatewayMsg::Embed(embed_res.proof.inner.as_ref().clone());
                                self.endpoint.send(embed_proof_msg).unwrap();

                                // Send Exit message to complete the workflow
                                self.endpoint.send(GatewayMsg::Exit).unwrap();
                                break; // Exit the worker loop
                            } else {
                                error!("[{}] failed to verify final embed proof", self.prover_id);
                            }
                        }
                        info!(
                            "[{}] send combine response of chunk-{}",
                            self.prover_id, &res.chunk_index,
                        );
                        let msg = GatewayMsg::Combine(CombineMsg::Response(res), task_id, ip_addr);
                        self.endpoint.send(msg).unwrap();
                    }
                    // Compress and embed phases are now handled directly in the combine phase
                    // No separate message handling needed
                    GatewayMsg::Exit => break,
                    _ => panic!("unsupported"),
                }

                // request for the next task
                let msg = GatewayMsg::RequestTask;
                self.endpoint.send(msg).unwrap();
            }
        })
    }

    fn run_cuda(self) -> JoinHandle<()> {
        unimplemented!()
    }
}

fn get_vk_root(vk_manager: &VkMerkleManager<SC>) -> [Val; DIGEST_SIZE] {
    if vk_manager.vk_verification_enabled() {
        vk_manager.merkle_root
    } else {
        [Val::ZERO; DIGEST_SIZE]
    }
}
