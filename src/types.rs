use pico_vm::configs::{
    config::StarkGenericConfig,
    stark_config::{KoalaBearBn254Poseidon2, KoalaBearPoseidon2},
};
use sqlx::{Pool, Sqlite};

pub type DbPool = Pool<Sqlite>;
pub type SC = KoalaBearPoseidon2;
pub type Val = <KoalaBearPoseidon2 as StarkGenericConfig>::Val;
pub type EmbedSC = KoalaBearBn254Poseidon2;
