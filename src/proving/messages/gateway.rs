use crate::{
    proving::messages::{combine::CombineMsg, riscv::RiscvMsg},
    types::EmbedSC,
};
use pico_vm::machine::proof::MetaProof;

type IpAddr = String;
type TaskId = String;

#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub enum GatewayMsg {
    // identify the emulator complete
    // TODO: add block number for multiple block proving
    EmulatorComplete,
    // request task by worker
    RequestTask,
    // riscv
    Riscv(RiscvMsg, TaskId, IpAddr),
    // combine
    Combine(CombineMsg, TaskId, IpAddr),
    // embed proof from direct execution
    Embed(MetaProof<EmbedSC>),
    // close a client by ip
    Close(IpAddr),
    // exit
    Exit,
}

impl GatewayMsg {
    pub fn ip_addr(&self) -> IpAddr {
        match self {
            Self::EmulatorComplete | Self::RequestTask | Self::Exit | Self::Embed(_) => "",
            Self::Riscv(_, _, ip_addr) => ip_addr,
            Self::Combine(_, _, ip_addr) => ip_addr,
            Self::Close(ip_addr) => ip_addr,
        }
        .to_string()
    }
}
