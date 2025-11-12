pub(crate) use crate::{
    proving::{
        gateway::handler::proof_tree::IndexedProof,
        messages::compress::{CompressRequest, CompressResponse},
    },
    types::{SC, Val},
};
use anyhow::Result;
use p3_field::FieldAlgebra;
use pico_perf::common::print_utils::log_section;
use pico_vm::{
    compiler::recursion::circuit::witness::Witnessable,
    configs::{config::Challenge, field_config::KoalaBearSimple},
    emulator::recursion::emulator::Runtime,
    instances::{
        chiptype::recursion_chiptype::RecursionChipType,
        compiler::{
            recursion_circuit::{
                compress::builder::CompressVerifierCircuit, stdin::RecursionStdin,
            },
            vk_merkle::{
                HasStaticVkManager, VkMerkleManager, builder::CompressVkVerifierCircuit,
                stdin::RecursionStdinVariant,
            },
        },
        machine::{combine::CombineMachine, compress::CompressMachine},
    },
    machine::{
        keys::HashableKey, machine::MachineBehavior, proof::MetaProof, witness::ProvingWitness,
    },
    primitives::consts::{DIGEST_SIZE, KOALABEAR_S_BOX_DEGREE, RECURSION_NUM_PVS},
};
use std::{sync::Arc, time::Instant};
use tracing::info;

pub struct CompressProver {
    prover_id: String,
    machine: CompressMachine<SC, RecursionChipType<Val>>,
}

impl CompressProver {
    pub fn new(prover_id: String) -> Self {
        let machine = CompressMachine::<_, _>::new(
            SC::compress(),
            RecursionChipType::<Val>::all_chips(),
            RECURSION_NUM_PVS,
        );

        Self { prover_id, machine }
    }
}

/// specialization for running prover on either babybear or koalabear
pub trait CompressHandler {
    fn process(&self, req: CompressRequest) -> CompressResponse;
    fn verify(&self, proof: &MetaProof<SC>, riscv_vk: &dyn HashableKey<Val>) -> Result<()>;
}

impl CompressHandler for CompressProver {
    fn process(&self, req: CompressRequest) -> CompressResponse {
        log_section("COMPRESS PHASE");

        let CompressRequest { chunk_index, proof } = req;

        info!(
            "[{}] receive compress request: chunk_index = {}",
            self.prover_id, chunk_index,
        );

        let start = Instant::now();

        // Get the vk manager and root
        let vk_manager = <SC as HasStaticVkManager>::static_vk_manager();
        let vk_root = get_vk_root(vk_manager);

        // Create the compress stdin from the combine proof
        // Note: We need to use the combine machine's base machine, not the compress machine's
        // This is because the compress circuit verifies the combine proof
        let combine_machine = CombineMachine::<_, _>::new(
            SC::default(),
            RecursionChipType::<Val>::all_chips(),
            RECURSION_NUM_PVS,
        );
        let combine_machine_base = combine_machine.base_machine();
        let compress_stdin = RecursionStdin::new(
            combine_machine_base,
            proof.get_inner().vks.clone(),
            proof.get_inner().proofs.clone(),
            true,
            vk_root,
        );

        // Build the compress program
        let (compress_program, compress_stdin_variant) = if vk_manager.vk_verification_enabled() {
            let compress_vk_stdin = vk_manager.add_vk_merkle_proof(compress_stdin);
            let mut compress_program = CompressVkVerifierCircuit::<KoalaBearSimple, SC>::build(
                combine_machine_base,
                &compress_vk_stdin,
            );
            compress_program.shape = Some(RecursionChipType::<Val>::compress_shape());

            (
                compress_program,
                RecursionStdinVariant::WithVk(compress_vk_stdin),
            )
        } else {
            let compress_program = CompressVerifierCircuit::<KoalaBearSimple, SC>::build(
                combine_machine_base,
                &compress_stdin,
            );

            (
                compress_program,
                RecursionStdinVariant::NoVk(compress_stdin),
            )
        };

        let compress_program = Arc::new(compress_program);

        // Emulate to get the record
        let mut record = {
            let mut witness_stream = Vec::new();
            Witnessable::<KoalaBearSimple>::write(&compress_stdin_variant, &mut witness_stream);
            let mut runtime = Runtime::<Val, Challenge<SC>, _, _, KOALABEAR_S_BOX_DEGREE>::new(
                compress_program.clone(),
                self.machine.config().perm.clone(),
            );
            runtime.witness_stream = witness_stream.into();
            runtime.run().unwrap();
            runtime.record
        };

        // Complement the record
        CompressMachine::<SC, _>::complement_record_static(self.machine.chips(), &mut record);

        // Setup keys and witness
        let (compress_pk, compress_vk) = self.machine.setup_keys(&compress_program);
        let compress_witness =
            ProvingWitness::setup_with_keys_and_records(compress_pk, compress_vk, vec![record]);

        // Generate the compress proof
        let compress_proof = self.machine.prove(&compress_witness);
        let compress_proof = IndexedProof::new(compress_proof, chunk_index, chunk_index);

        info!(
            "[{}] finish compress proving chunk-{chunk_index}, time used: {}ms",
            self.prover_id,
            start.elapsed().as_millis()
        );

        CompressResponse {
            chunk_index,
            proof: compress_proof,
        }
    }

    fn verify(&self, proof: &MetaProof<SC>, riscv_vk: &dyn HashableKey<Val>) -> Result<()> {
        self.machine.verify(proof, riscv_vk)
    }
}

fn get_vk_root(vk_manager: &VkMerkleManager<SC>) -> [Val; DIGEST_SIZE] {
    if vk_manager.vk_verification_enabled() {
        vk_manager.merkle_root
    } else {
        [Val::ZERO; DIGEST_SIZE]
    }
}
