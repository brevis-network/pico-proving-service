use crate::{
    proving::{
        gateway::handler::proof_tree::IndexedProof,
        messages::combine::{CombineRequest, CombineResponse},
    },
    types::{SC, Val},
};
use anyhow::Result;
use p3_koala_bear::KoalaBear;
use pico_perf::common::print_utils::log_section;
use pico_vm::{
    configs::stark_config::KoalaBearPoseidon2,
    instances::{
        chiptype::recursion_chiptype::RecursionChipType, machine::combine::CombineMachine,
    },
    machine::{keys::HashableKey, machine::MachineBehavior, proof::MetaProof},
    primitives::consts::{COMBINE_SIZE, RECURSION_NUM_PVS},
};
use tracing::info;

pub struct CombineProver {
    prover_id: String,
    machine: CombineMachine<SC, RecursionChipType<Val>>,
}

impl CombineProver {
    pub fn new(prover_id: String) -> Self {
        let machine = CombineMachine::<_, _>::new(
            SC::default(),
            RecursionChipType::<Val>::all_chips(),
            RECURSION_NUM_PVS,
        );

        Self { prover_id, machine }
    }
}

/// specialization for running prover on either babybear or koalabear
pub trait CombineHandler {
    fn process(&self, req: CombineRequest) -> CombineResponse;
    fn verify(&self, proof: &MetaProof<SC>, riscv_vk: &dyn HashableKey<Val>) -> Result<()>;
}

impl CombineHandler for CombineProver {
    fn process(&self, req: CombineRequest) -> CombineResponse {
        log_section("COMBINE PHASE");

        let CombineRequest {
            chunk_index,
            flag_complete,
            proofs,
        } = req;
        assert!(proofs.len() <= COMBINE_SIZE);

        info!(
            "[{}] receive combine request: chunk_index = {}",
            self.prover_id, chunk_index,
        );

        let start_a = proofs[0].start_chunk;
        let end_a = proofs[0].end_chunk;

        let start_b = proofs[1].start_chunk;
        let end_b = proofs[1].end_chunk;

        assert_eq!(
            end_a + 1,
            start_b,
            "proofs are not adjacent: cannot combine"
        );

        let meta_a = proofs[0].get_inner().clone();
        let meta_b = proofs[1].get_inner().clone();

        let proof = self.machine.prove_two(meta_a, meta_b, flag_complete);
        let proof = IndexedProof::new(proof, start_a, end_b);

        CombineResponse { chunk_index, proof }
    }

    fn verify(
        &self,
        proof: &MetaProof<KoalaBearPoseidon2>,
        riscv_vk: &dyn HashableKey<KoalaBear>,
    ) -> Result<()> {
        self.machine.verify(proof, riscv_vk)
    }
}
