use crate::{
    proving::gateway::handler::proof_tree::IndexedProof,
    types::{EmbedSC, SC},
};
use derive_more::Constructor;
use pico_vm::machine::proof::MetaProof;

#[derive(Clone, Constructor)]
pub struct EmbedRequest {
    pub chunk_index: usize,
    pub proof: IndexedProof<MetaProof<SC>>,
}

#[derive(Clone, Constructor)]
pub struct EmbedResponse {
    pub chunk_index: usize,
    pub proof: IndexedProof<MetaProof<EmbedSC>>,
}
