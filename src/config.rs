use crate::{
    impl_auth_config,
    utils::auth::{AuthConfig, AuthMethod},
};
use clap::Parser;
use std::net::SocketAddr;

#[derive(Debug, Parser, Clone)]
#[clap(author, version, about, long_about = None)]
pub struct ServiceConfig {
    #[clap(
        long,
        env = "DATABASE_URL",
        default_value = "sqlite://pico_proving_service.db",
        help = "Local database URL"
    )]
    pub db_url: String,

    #[clap(
        long,
        env = "GRPC_ADDR",
        default_value = "[::]:50052",
        help = "gRPC listen address"
    )]
    pub grpc_addr: SocketAddr,

    #[clap(
        long,
        env = "AUTH_METHOD",
        default_value = "none",
        value_enum,
        help = "Authentication method (none, bearer)"
    )]
    pub auth_method: AuthMethod,

    #[clap(
        long,
        env = "BEARER_TOKEN",
        requires = "auth_method",
        help = "Bearer token (required if auth_method=bearer)"
    )]
    pub bearer_token: Option<String>,

    #[clap(
        long,
        env = "MAX_GRPC_MSG_SIZE",
        default_value = "1073741824",
        help = "Max gRPC message size (bytes)"
    )]
    pub max_grpc_msg_size: usize,

    #[clap(
        long,
        env = "PROVER_COUNT",
        default_value = "1",
        help = "Prover count to start"
    )]
    pub prover_count: usize,

    #[clap(
        long,
        env = "MAX_EMULATION_CYCLES",
        help = "maximum supported emulation cycles"
    )]
    pub max_emulation_cycles: Option<u64>,
}

impl_auth_config!(ServiceConfig);

impl ServiceConfig {
    pub fn validate(&self) -> Result<(), String> {
        self.validate_auth()
    }
}
