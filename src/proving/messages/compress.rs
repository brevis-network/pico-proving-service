use crate::{proving::gateway::handler::proof_tree::IndexedProof, types::SC};
use derive_more::Constructor;
use pico_vm::machine::proof::MetaProof;

#[derive(Clone, Constructor)]
pub struct CompressRequest {
    pub chunk_index: usize,
    pub proof: IndexedProof<MetaProof<SC>>,
}

#[derive(Clone, Constructor)]
pub struct CompressResponse {
    pub chunk_index: usize,
    pub proof: IndexedProof<MetaProof<SC>>,
}
