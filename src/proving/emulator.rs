use crate::{
    proving::messages::{
        gateway::GatewayMsg,
        riscv::{RiscvMsg, RiscvRequest},
    },
    proving_queue::ProvingTask,
    types::SC,
};
use anyhow::Result;
use crossbeam::channel::{Receiver, Sender, bounded};
use log::{debug, info};
use p3_koala_bear::KoalaBear;
use pico_perf::common::print_utils::log_section;
use pico_vm::{
    compiler::riscv::program::Program,
    configs::{config::StarkGenericConfig, stark_config::kb_poseidon2::KoalaBearPoseidon2},
    emulator::{emulator::MetaEmulator, opts::EmulatorOpts, stdin::EmulatorStdin},
    instances::{
        chiptype::riscv_chiptype::RiscvChipType, compiler::vk_merkle::HasStaticVkManager,
        configs::riscv_kb_config::StarkConfig as RiscvKBSC, machine::riscv::RiscvMachine,
    },
    machine::{machine::MachineBehavior, witness::ProvingWitness},
    primitives::consts::RISCV_NUM_PVS,
};
use std::{sync::Arc, thread, time::Instant};

pub trait EmulatorRunner: StarkGenericConfig {
    fn run(task: ProvingTask, gateway_endpoint: Arc<Sender<GatewayMsg>>) -> Result<()>;
}

impl EmulatorRunner for KoalaBearPoseidon2 {
    fn run(task: ProvingTask, gateway_endpoint: Arc<Sender<GatewayMsg>>) -> Result<()> {
        // Setups
        let _vk_manager = <KoalaBearPoseidon2 as HasStaticVkManager>::static_vk_manager();

        let riscv_machine =
            RiscvMachine::new(RiscvKBSC::new(), RiscvChipType::all_chips(), RISCV_NUM_PVS);

        // Use the program directly from the task since it's already been preprocessed
        // The program in ProvingTask has already been compiled and preprocessed
        let program = task.program.clone();

        // Create stdin from inputs
        let stdin_builder = task.inputs.map_or_else(
            || EmulatorStdin::<Program, Vec<u8>>::new_builder::<KoalaBearPoseidon2>(),
            |inputs| bincode::deserialize(&inputs).unwrap(),
        );
        let (stdin, _) = stdin_builder.finalize::<Program>();

        let (pk, vk) = riscv_machine.setup_keys(&program);

        let riscv_opts = EmulatorOpts::bench_riscv_ops();
        let witness =
            ProvingWitness::<KoalaBearPoseidon2, RiscvChipType<KoalaBear>, _>::setup_for_riscv(
                program,
                stdin,
                riscv_opts,
                pk.clone(),
                vk.clone(),
            );
        // Initialize the emulator.
        let mut emulator = MetaEmulator::setup_riscv(&witness, None);

        let channel_capacity = (4 * witness
            .opts
            .as_ref()
            .map(|opts| opts.chunk_batch_size)
            .unwrap_or(64)) as usize;
        // Initialize the channel for sending emulation records from the emulator thread to prover.
        let (record_sender, record_receiver): (Sender<_>, Receiver<_>) = bounded(channel_capacity);

        // Start the emulator thread.
        log_section("RISCV EMULATE PHASE");
        let emulator_handle = thread::spawn(move || {
            let mut batch_num = 1;
            loop {
                let start_local = Instant::now();

                let report = emulator.next_record_batch(&mut |record| {
                    record_sender.send(record).expect(
                        "Failed to send an emulation record from emulator thread to prover thread",
                    )
                });

                tracing::debug!(
                    "--- Generate riscv records for batch-{} in {:?}",
                    batch_num,
                    start_local.elapsed(),
                );

                if report.done {
                    break;
                }

                batch_num += 1;
            }

            // Move and return the emulator for further usage.
            emulator

            // `record_sender` will be dropped when the emulator thread completes.
        });

        // RISCV Phase
        log_section("RISCV & CONVERT PHASE");
        let mut chunk_index = 0;

        while let Ok(record) = record_receiver.recv() {
            let req = RiscvRequest {
                chunk_index,
                record,
            };

            tracing::debug!("send emulation record-{chunk_index}");
            gateway_endpoint.send(GatewayMsg::Riscv(
                RiscvMsg::Request(req),
                // TODO: fix to id and ip address
                chunk_index.to_string(),
                "".to_string(),
            ))?;

            chunk_index += 1;
        }

        // send the emulator complete message
        gateway_endpoint.send(GatewayMsg::EmulatorComplete)?;

        let emulator = emulator_handle.join().unwrap();
        info!("Total Cycles: {}", emulator.cycles());

        Ok(())
    }
}

pub fn run(task: ProvingTask, gateway_endpoint: Arc<Sender<GatewayMsg>>) {
    debug!("[coordinator] emulator init");
    SC::run(task.clone(), gateway_endpoint.clone()).unwrap();
    debug!("[coordinator] emulator run completed");
}
