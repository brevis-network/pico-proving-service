use crate::{
    error::PicoError,
    types::{SC, Val},
};
use alloy_primitives::U256;
use std::collections::HashMap;
use pico_vm::{
    compiler::riscv::program::Program,
    emulator::{
        opts::EmulatorOpts,
        stdin::{EmulatorStdin, EmulatorStdinBuilder},
    },
    instances::chiptype::riscv_chiptype::RiscvChipType,
    machine::{
        estimator::EstimatorModel,
        keys::{BaseProvingKey, BaseVerifyingKey},
        witness::ProvingWitness,
    },
    proverchain::emulate_snapshot_pipeline,
};
use sha2::{Digest, Sha256};
use std::{panic, sync::Arc, sync::Mutex};

pub struct EstimatedInfo {
    pub cost: u64,
    pub total_cycles: u64,
    pub pv_digest: U256,
    pub pv_stream: Vec<u8>,
    pub precompile_counts: HashMap<String, u64>,
}

pub fn estimate_cost(
    program: Arc<Program>,
    pk: BaseProvingKey<SC>,
    vk: BaseVerifyingKey<SC>,
    inputs: Option<&[u8]>,
    max_cycles: Option<u64>,
    cost_estimator: bool,
    raw_input: bool,
) -> Result<EstimatedInfo, PicoError> {
    let res = panic::catch_unwind(|| {
        // deserialize stdin builder
        let stdin_builder: EmulatorStdinBuilder<Vec<u8>, SC> = if raw_input {
            // raw_input mode: wrap raw bytes with write_slice
            let mut builder = EmulatorStdin::<Program, Vec<u8>>::new_builder::<SC>();
            if let Some(bytes) = inputs {
                builder.write_slice(bytes);
            }
            builder
        } else {
            // original mode: bincode deserialize
            inputs.map_or_else(
                EmulatorStdin::<Program, Vec<u8>>::new_builder::<SC>,
                |inputs| bincode::deserialize(inputs).unwrap(),
            )
        };

        let (stdin, _) = stdin_builder.finalize::<Program>();

        let opts = if cost_estimator {
            EmulatorOpts::bench_riscv_ops().with_cost_estimator()
        } else {
            EmulatorOpts::bench_riscv_ops()
        };
        let opts = match max_cycles {
            Some(max_cycles) => opts.with_max_cycles(max_cycles),
            None => opts,
        };
        let witness = ProvingWitness::<SC, RiscvChipType<Val>, _>::setup_for_riscv(
            program, stdin, opts, pk, vk,
        );

        // Collect precompile event counts from each EmulationRecord
        let precompile_counts = Arc::new(Mutex::new(HashMap::<String, u64>::new()));
        let counts_ref = precompile_counts.clone();

        let (reports, total_cycles, pv_stream) =
            emulate_snapshot_pipeline(&witness, move |rec, _done| {
                let mut counts = counts_ref.lock().unwrap();
                for (syscall_code, events) in rec.precompile_events.events.iter() {
                    if !events.is_empty() {
                        *counts.entry(format!("{:?}", syscall_code)).or_insert(0) +=
                            events.len() as u64;
                    }
                }
            })?;

        let cost = if cost_estimator {
            let model = EstimatorModel::from_json("fixtures/model.json");
            let estimators = reports
                .into_iter()
                .map(|r| r.host_cycle_estimator.unwrap().into_iter())
                .flatten();

            estimators.map(|e| e.estimate(&model)).sum::<usize>() as u64
        } else {
            total_cycles
        };

        let mut pv_digest = U256::from_be_bytes(sha256(&pv_stream));
        let mask = (U256::ONE << 253) - U256::ONE;
        pv_digest &= mask;

        let precompile_counts = Arc::try_unwrap(precompile_counts)
            .expect("precompile_counts Arc still has references")
            .into_inner()
            .unwrap();

        Ok(EstimatedInfo {
            cost,
            total_cycles,
            pv_digest,
            pv_stream,
            precompile_counts,
        })
    });

    match res {
        Ok(Ok(info)) => Ok(info),
        Ok(Err(e)) => Err(e),
        Err(e) => Err(PicoError::InternalError(format!(
            "panic during cost estimation {e:?}"
        ))),
    }
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}
