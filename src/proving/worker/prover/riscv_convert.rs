use super::VkRoot;
use crate::{
    proving::{
        gateway::handler::proof_tree::IndexedProof,
        messages::riscv::{RiscvRequest, RiscvResponse},
    },
    proving_queue::ProvingTask,
    types::{SC, Val},
};
use log::debug;
use p3_field::FieldAlgebra;
use p3_koala_bear::KoalaBear;
use pico_perf::common::print_utils::log_section;
use pico_vm::{
    configs::{
        config::{FieldGenericConfig, StarkGenericConfig},
        field_config::KoalaBearSimple,
        stark_config::KoalaBearPoseidon2,
    },
    emulator::{opts::EmulatorOpts, stdin::EmulatorStdin},
    instances::{
        chiptype::{recursion_chiptype::RecursionChipType, riscv_chiptype::RiscvChipType},
        compiler::{
            shapes::{recursion_shape::RecursionShapeConfig, riscv_shape::RiscvShapeConfig},
            vk_merkle::HasStaticVkManager,
        },
        machine::{convert::ConvertMachine, riscv::RiscvMachine},
    },
    machine::{
        keys::{BaseProvingKey, BaseVerifyingKey},
        machine::MachineBehavior,
        witness::ProvingWitness,
    },
    primitives::consts::{DIGEST_SIZE, RECURSION_NUM_PVS, RISCV_NUM_PVS},
};
use std::time::Instant;
use tracing::info;

pub struct RiscvConvertProver {
    prover_id: String,
    riscv_shape_config: Option<RiscvShapeConfig<Val>>,
    recursion_shape_config: Option<RecursionShapeConfig<Val, RecursionChipType<Val>>>,
    riscv_machine: RiscvMachine<SC, RiscvChipType<Val>>,
    convert_machine: ConvertMachine<SC, RecursionChipType<Val>>,
    pk: BaseProvingKey<SC>,
    riscv_vk: BaseVerifyingKey<SC>,
}

impl RiscvConvertProver {
    pub fn new(prover_id: String, task: ProvingTask) -> Self {
        // opts and setups
        let vk_manager = <SC as HasStaticVkManager>::static_vk_manager();
        let vk_enabled = vk_manager.vk_verification_enabled();
        let riscv_shape_config = if vk_enabled {
            Some(RiscvShapeConfig::<Val>::default())
        } else {
            None
        };
        let recursion_shape_config = if vk_enabled {
            Some(RecursionShapeConfig::<Val, RecursionChipType<Val>>::default())
        } else {
            None
        };
        // let (elf, _) = load::<Program, SC>(&program).unwrap();

        let riscv_machine =
            RiscvMachine::new(SC::default(), RiscvChipType::all_chips(), RISCV_NUM_PVS);

        // Use the program directly from the task since it's already been preprocessed
        // The program in ProvingTask has already been compiled and preprocessed
        // let riscv_program = task.program.clone();

        // TODO: see whether to use pk and vk from ProvingTask
        // let (pk, riscv_vk) = riscv_machine.setup_keys(&riscv_program);

        let convert_machine = ConvertMachine::new(
            SC::default(),
            RecursionChipType::<Val>::all_chips(),
            RECURSION_NUM_PVS,
        );

        Self {
            prover_id,
            riscv_shape_config,
            recursion_shape_config,
            riscv_machine,
            convert_machine,
            pk: task.pk.as_ref().clone(),
            riscv_vk: task.vk.as_ref().clone(),
        }
    }

    pub fn riscv_vk(&self) -> &BaseVerifyingKey<SC> {
        &self.riscv_vk
    }
}

/// specialization for running prover on either babybear or koalabear
pub trait RiscvConvertHandler {
    fn process(&self, req: RiscvRequest, vk_root: &VkRoot) -> RiscvResponse;
}

impl RiscvConvertHandler for RiscvConvertProver {
    fn process(&self, req: RiscvRequest, vk_root: &VkRoot) -> RiscvResponse {
        log_section("RISCV PHASE");

        let mut challenger = self.riscv_machine.config().challenger().clone();
        self.pk.observed_by(&mut challenger);

        let chunk_index = req.chunk_index;
        let is_last_chunk = req.record.is_last;

        info!(
            "[{}] receive riscv-convert request: chunk_index = {}",
            self.prover_id, chunk_index,
        );

        let start = Instant::now();

        let proof = self.riscv_machine.prove_record(
            chunk_index,
            &self.pk,
            &challenger,
            self.riscv_shape_config.as_ref(),
            req.record,
        );

        info!("RISCV Phase complete! chunk_index: {}", chunk_index);

        log_section("CONVERT PHASE");

        let recursion_opts = EmulatorOpts::default();
        debug!("recursion_opts: {:?}", recursion_opts);

        let convert_stdin = EmulatorStdin::setup_for_convert_with_index::<
            <KoalaBearSimple as FieldGenericConfig>::F,
            KoalaBearSimple,
        >(
            &self.riscv_vk,
            *vk_root,
            [KoalaBear::ZERO; DIGEST_SIZE],
            self.riscv_machine.base_machine(),
            &proof,
            &self.recursion_shape_config,
            chunk_index,
            is_last_chunk,
        );
        let convert_witness = ProvingWitness::setup_for_convert(
            convert_stdin,
            KoalaBearPoseidon2::new().into(),
            recursion_opts,
        );

        let proof = self
            .convert_machine
            .prove_with_index(chunk_index as u32, &convert_witness);
        let proof = IndexedProof::new(proof, chunk_index, chunk_index);

        info!(
            "[worker] finish proving chunk-{chunk_index}, time used: {}ms",
            start.elapsed().as_millis()
        );

        // return the riscv-convert result
        RiscvResponse { chunk_index, proof }
    }
}
