pub(crate) mod proof_tree;

use crate::{
    proving::messages::{
        combine::{CombineMsg, CombineRequest, CombineResponse},
        gateway::GatewayMsg,
        riscv::{RiscvMsg, RiscvRequest, RiscvResponse},
    },
    types::{EmbedSC, SC},
};
use anyhow::Result;
use pico_vm::machine::proof::MetaProof;
use proof_tree::ProofTree;
use tracing::info;

pub struct GatewayHandler {
    // identify if emulation is complete, it could be used to check if the leaves are complete in
    // proof tree
    emulator_complete: bool,
    proof_tree: ProofTree<MetaProof<SC>>,
    // store the embed proof result
    embed_proof: Option<MetaProof<EmbedSC>>,
}

impl GatewayHandler {
    pub fn new() -> Self {
        Self {
            emulator_complete: false,
            proof_tree: ProofTree::default(),
            embed_proof: None,
        }
    }

    pub fn complete(&self) -> bool {
        self.embed_proof.is_some()
    }

    pub fn get_embed_proof(&self) -> Option<MetaProof<EmbedSC>> {
        self.embed_proof.clone()
    }

    pub fn set_embed_proof(&mut self, proof: MetaProof<EmbedSC>) {
        self.embed_proof = Some(proof);
        info!(
            "[gateway] embed proof stored, size: {} bytes",
            bincode::serialize(&self.embed_proof.as_ref().unwrap())
                .unwrap()
                .len()
        );
    }

    pub fn process_riscv_req(&mut self, msg: &GatewayMsg) -> Result<Option<GatewayMsg>> {
        match msg {
            GatewayMsg::Riscv(msg, _, _) => match msg {
                RiscvMsg::Request(RiscvRequest { chunk_index, .. }) => {
                    // save the placeholder for the processing proof
                    self.proof_tree.init_node(*chunk_index);
                }
                _ => {
                    panic!("unexpected message in process_riscv_req");
                }
            },
            _ => panic!("unsupported"),
        }
        Ok(None)
    }
    pub fn process(&mut self, msg: GatewayMsg) -> Result<Option<GatewayMsg>> {
        let mut index_proofs_to_combine = None;
        match msg {
            GatewayMsg::EmulatorComplete => self.emulator_complete = true,
            GatewayMsg::Riscv(msg, _, _) => match msg {
                RiscvMsg::Request(RiscvRequest { chunk_index, .. }) => {
                    // save the placeholder for the processing proof
                    self.proof_tree.init_node(chunk_index);
                }
                RiscvMsg::Response(RiscvResponse { chunk_index, proof }) => {
                    index_proofs_to_combine = self
                        .proof_tree
                        .set_proof(chunk_index, proof)
                        .map(|proofs| (chunk_index, proofs));
                }
            },
            GatewayMsg::Combine(
                CombineMsg::Response(CombineResponse { chunk_index, proof }),
                _,
                _,
            ) => {
                index_proofs_to_combine = self
                    .proof_tree
                    .set_proof(chunk_index, proof)
                    .map(|proofs| (chunk_index, proofs));
            }
            GatewayMsg::Embed(proof) => {
                // Store the embed proof directly from worker prover
                self.set_embed_proof(proof);
                info!("[gateway] received embed proof from worker prover");
            }
            // Compress and embed phases are now handled directly in worker provers
            // No message handling needed here
            _ => panic!("unsupported"),
        }

        // Check if proving is completely done (embed completed)
        if self.complete() {
            info!("[gateway] proving complete");

            // Always exit when proving is complete
            return Ok(Some(GatewayMsg::Exit));
        }

        if let Some((chunk_index, proofs)) = index_proofs_to_combine {
            assert_eq!(proofs.len(), 2);

            // return the combine message
            return Ok(Some(GatewayMsg::Combine(
                CombineMsg::Request(CombineRequest {
                    flag_complete: self.proof_tree.len() == 1, // TODO: check this
                    chunk_index,
                    proofs,
                }),
                chunk_index.to_string(),
                "".to_string(),
            )));
        }

        Ok(None)
    }
}
