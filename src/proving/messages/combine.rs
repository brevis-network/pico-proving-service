use crate::{proving::gateway::handler::proof_tree::IndexedProof, types::SC};
use derive_more::Constructor;
use pico_vm::machine::proof::MetaProof;

#[derive(Clone)]
pub enum CombineMsg {
    Request(CombineRequest),
    Response(CombineResponse),
}

#[derive(Clone, Constructor)]
pub struct CombineRequest {
    pub flag_complete: bool,
    pub chunk_index: usize,
    pub proofs: Vec<IndexedProof<MetaProof<SC>>>,
}

#[derive(Clone, Constructor)]
pub struct CombineResponse {
    pub chunk_index: usize,
    pub proof: IndexedProof<MetaProof<SC>>,
}
