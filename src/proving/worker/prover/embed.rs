use crate::{
    proving::{
        gateway::handler::proof_tree::IndexedProof,
        messages::embed::{EmbedRequest, EmbedResponse},
    },
    types::{EmbedSC, SC, Val},
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
            recursion_circuit::{embed::builder::EmbedVerifierCircuit, stdin::RecursionStdin},
            vk_merkle::{
                HasStaticVkManager, VkMerkleManager, builder::EmbedVkVerifierCircuit,
                stdin::RecursionStdinVariant,
            },
        },
        machine::{compress::CompressMachine, embed::EmbedMachine},
    },
    machine::{
        keys::HashableKey, machine::MachineBehavior, proof::MetaProof, witness::ProvingWitness,
    },
    primitives::consts::{DIGEST_SIZE, KOALABEAR_S_BOX_DEGREE, RECURSION_NUM_PVS},
};
use std::{sync::Arc, time::Instant};
use tracing::info;

pub struct EmbedProver {
    prover_id: String,
    machine: EmbedMachine<SC, EmbedSC, RecursionChipType<Val>, Vec<u8>>,
}

impl EmbedProver {
    pub fn new(prover_id: String) -> Self {
        let machine = EmbedMachine::<_, _, _, _>::new(
            EmbedSC::default(),
            RecursionChipType::<Val>::embed_chips(),
            RECURSION_NUM_PVS,
        );

        Self { prover_id, machine }
    }
}

/// specialization for running prover on either babybear or koalabear
pub trait EmbedHandler {
    fn process(&self, req: EmbedRequest) -> EmbedResponse;
    fn verify(&self, proof: &MetaProof<EmbedSC>, riscv_vk: &dyn HashableKey<Val>) -> Result<()>;
}

impl EmbedHandler for EmbedProver {
    fn process(&self, req: EmbedRequest) -> EmbedResponse {
        log_section("EMBED PHASE");

        let EmbedRequest { chunk_index, proof } = req;

        info!(
            "[{}] receive embed request: chunk_index = {}",
            self.prover_id, chunk_index,
        );

        let start = Instant::now();

        // Get the vk manager and root
        let vk_manager = <SC as HasStaticVkManager>::static_vk_manager();
        let vk_root = get_vk_root(vk_manager);

        // Create the embed stdin from the compress proof
        // Note: We need to use the compress machine's base machine, not the embed machine's
        // This is because the embed circuit verifies the compress proof
        let compress_machine = CompressMachine::<_, _>::new(
            SC::compress(),
            RecursionChipType::<Val>::compress_chips(),
            RECURSION_NUM_PVS,
        );
        let compress_machine_base = compress_machine.base_machine();
        let embed_stdin = RecursionStdin::new(
            compress_machine_base,
            proof.get_inner().vks.clone(),
            proof.get_inner().proofs.clone(),
            true,
            vk_root,
        );

        // Build the embed program
        let (embed_program, embed_stdin_variant) = if vk_manager.vk_verification_enabled() {
            let embed_vk_stdin = vk_manager.add_vk_merkle_proof(embed_stdin);
            let embed_program = EmbedVkVerifierCircuit::<KoalaBearSimple, SC>::build(
                compress_machine_base,
                &embed_vk_stdin,
                vk_manager,
            );

            (embed_program, RecursionStdinVariant::WithVk(embed_vk_stdin))
        } else {
            let embed_program = EmbedVerifierCircuit::<KoalaBearSimple, SC>::build(
                compress_machine_base,
                &embed_stdin,
            );

            (embed_program, RecursionStdinVariant::NoVk(embed_stdin))
        };

        let embed_program = Arc::new(embed_program);

        // Emulate to get the record
        let mut record = {
            let mut witness_stream = Vec::new();
            Witnessable::<KoalaBearSimple>::write(&embed_stdin_variant, &mut witness_stream);
            let mut runtime = Runtime::<Val, Challenge<EmbedSC>, _, _, KOALABEAR_S_BOX_DEGREE>::new(
                embed_program.clone(),
                compress_machine.config().perm.clone(),
            );
            runtime.witness_stream = witness_stream.into();
            runtime.run().unwrap();
            runtime.record
        };

        // Complement the record
        EmbedMachine::<SC, EmbedSC, _, Vec<u8>>::complement_record_static(
            self.machine.chips(),
            &mut record,
        );

        // Setup keys and witness
        let (embed_pk, embed_vk) = self.machine.setup_keys(&embed_program);
        let embed_witness =
            ProvingWitness::setup_with_keys_and_records(embed_pk, embed_vk, vec![record]);

        // Generate the embed proof
        let embed_proof = self.machine.prove(&embed_witness);
        let embed_proof = IndexedProof::new(embed_proof, chunk_index, chunk_index);

        info!(
            "[{}] finish embed proving chunk-{chunk_index}, time used: {}ms",
            self.prover_id,
            start.elapsed().as_millis()
        );

        EmbedResponse {
            chunk_index,
            proof: embed_proof,
        }
    }

    fn verify(&self, proof: &MetaProof<EmbedSC>, riscv_vk: &dyn HashableKey<Val>) -> Result<()> {
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
