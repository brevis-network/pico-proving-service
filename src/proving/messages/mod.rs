pub(crate) mod combine;
pub(crate) mod compress;
pub(crate) mod embed;
pub mod gateway;
pub(crate) mod riscv;
// compress is not required since it's handled right after final combine proof in the worker prover
