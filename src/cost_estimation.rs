use crate::{
    error::PicoError,
    types::{SC, Val},
};
use alloy_primitives::U256;
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
use std::{panic, sync::Arc};

pub struct EstimatedInfo {
    pub cost: u64,
    pub total_cycles: u64,
    pub pv_digest: U256,
}

pub fn estimate_cost(
    program: Arc<Program>,
    pk: BaseProvingKey<SC>,
    vk: BaseVerifyingKey<SC>,
    inputs: Option<&[u8]>,
    max_cycles: Option<u64>,
    cost_estimator: bool,
) -> Result<EstimatedInfo, PicoError> {
    let res = panic::catch_unwind(|| {
        // deserialize stdin builder
        let stdin_builder: EmulatorStdinBuilder<Vec<u8>, SC> = inputs.map_or_else(
            EmulatorStdin::<Program, Vec<u8>>::new_builder::<SC>,
            |inputs| bincode::deserialize(inputs).unwrap(),
        );

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

        let (reports, total_cycles, pv_stream) = emulate_snapshot_pipeline(&witness, |_, _| {})?;

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

        Ok(EstimatedInfo {
            cost,
            total_cycles,
            pv_digest,
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
