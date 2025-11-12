use crate::{ErrCode, ErrMsg, EstimateCostResponse};
use pico_vm::emulator::riscv::emulator::EmulationError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, Serialize, Deserialize)]
pub enum PicoError {
    // provided input exceeded the allowed cycle limit
    #[error("input exceeded cycle limit of {0}")]
    ExceededCycleLimit(u64),

    // common internal error
    #[error("internal error: {0}")]
    InternalError(String),
}

impl From<EmulationError> for PicoError {
    fn from(e: EmulationError) -> Self {
        match e {
            EmulationError::ExceededCycleLimit(cycles) => Self::ExceededCycleLimit(cycles),
            _ => Self::InternalError(e.to_string()),
        }
    }
}

impl From<PicoError> for EstimateCostResponse {
    fn from(e: PicoError) -> Self {
        match e {
            PicoError::ExceededCycleLimit(_) => {
                let err = Some(ErrMsg {
                    code: ErrCode::InputExceeded.into(),
                    msg: Some(e.to_string()),
                });
                Self {
                    err,
                    cost: 0,
                    pv_digest: vec![],
                }
            }
            PicoError::InternalError(_) => {
                let err = Some(ErrMsg {
                    code: ErrCode::Internal.into(),
                    msg: Some(e.to_string()),
                });
                Self {
                    err,
                    cost: 0,
                    pv_digest: vec![],
                }
            }
        }
    }
}
