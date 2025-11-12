use crate::{proving::gateway::handler::proof_tree::IndexedProof, types::SC};
use derive_more::Constructor;
use pico_vm::{emulator::riscv::record::EmulationRecord, machine::proof::MetaProof};

// TODO: rename
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub enum RiscvMsg {
    Request(RiscvRequest),
    Response(RiscvResponse),
}

#[derive(Clone, Constructor)]
pub struct RiscvRequest {
    // TODO: add identifier
    pub chunk_index: usize,
    pub record: EmulationRecord,
}

#[derive(Clone, Constructor)]
pub struct RiscvResponse {
    // TODO: add identifier
    pub chunk_index: usize,
    pub proof: IndexedProof<MetaProof<SC>>,
}
