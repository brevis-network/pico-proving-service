use crate::proving::messages::gateway::GatewayMsg;
use pico_vm::thread::channel::DuplexUnboundedEndpoint;

pub mod prover;

pub type WorkerEndpoint = DuplexUnboundedEndpoint<GatewayMsg, GatewayMsg>;
