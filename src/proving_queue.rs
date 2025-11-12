use crate::{
    config::ServiceConfig,
    proving,
    types::{DbPool, SC},
};
use crossbeam::channel::Receiver;
use dashmap::DashMap;
use derive_more::Constructor;
use pico_vm::{
    compiler::riscv::program::Program,
    machine::keys::{BaseProvingKey, BaseVerifyingKey},
};
use std::sync::Arc;
use tokio::{task::JoinHandle, time::Instant};
use tracing::{error, info};

#[derive(Constructor, Debug, Eq, Hash, PartialEq, Clone)]
pub struct ProvingKey {
    app_id: String,
    task_id: String,
}

impl ProvingKey {
    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    pub fn task_id(&self) -> &str {
        &self.task_id
    }
}

#[derive(Constructor, Clone)]
pub struct ProvingTask {
    pub key: ProvingKey,
    pub program: Arc<Program>,
    pub pk: Arc<BaseProvingKey<SC>>,
    pub vk: Arc<BaseVerifyingKey<SC>>,
    pub inputs: Option<Vec<u8>>,
    pub use_gpu: bool,
}

#[derive(Constructor)]
pub struct ProvingOutput {
    pub proof: Arc<[u8]>,
}

pub type ProvingOutputs = DashMap<ProvingKey, ProvingOutput>;

#[derive(Constructor)]
pub struct ProvingQueue {
    cfg: ServiceConfig,
    outputs: Arc<ProvingOutputs>,
    receiver: Arc<Receiver<ProvingTask>>,
    db_pool: Arc<DbPool>,
}

impl ProvingQueue {
    pub fn pop_output(&self, key: &ProvingKey) -> Option<ProvingOutput> {
        self.outputs.remove(key).map(|(_, v)| v)
    }

    pub fn run(&self) -> JoinHandle<()> {
        info!("[proving-network] proving queue init");

        let cfg = self.cfg.clone();
        let receiver = self.receiver.clone();
        let outputs = self.outputs.clone();
        let db_pool = self.db_pool.clone();

        let handle = tokio::spawn(async move {
            loop {
                let task = match tokio::task::block_in_place(|| receiver.recv()) {
                    Ok(task) => task,
                    Err(_) => {
                        info!("[proving-network] channel closed, exiting queue loop");
                        break;
                    }
                };
                let task_key = task.key.clone();
                info!("[proving-network] starting proving task: {:?}", task_key);

                // Run the real proving workflow with database pool
                info!("[proving-network] calling prove_task for: {:?}", task_key);
                let start = Instant::now();
                let result = proving::prove_task(task, cfg.prover_count).await;
                info!(
                    "[proving-network] prove_task returned for {:?}, proving time : {}",
                    task_key,
                    start.elapsed().as_secs_f32(),
                );

                match result {
                    Ok(proof_bytes) => {
                        info!(
                            "[proving-network] proving completed successfully for task: {:?}, proof size: {} bytes",
                            task_key,
                            proof_bytes.len()
                        );

                        // Store proof in memory for quick access
                        let proof_arc: Arc<[u8]> = Arc::from(proof_bytes);
                        let output = ProvingOutput::new(proof_arc.clone());
                        let _ = outputs.insert(task_key.clone(), output);
                        info!(
                            "[proving-network] proof stored in memory for task: {:?}, total memory entries: {}",
                            task_key,
                            outputs.len()
                        );

                        // Store proof in database
                        if let Err(e) =
                            Self::store_proof_in_db(&db_pool, &task_key, &proof_arc).await
                        {
                            error!(
                                "[proving-network] failed to store proof in database for task {:?}: {}",
                                task_key, e
                            );
                        } else {
                            info!(
                                "[proving-network] proof stored in database for task: {:?}",
                                task_key
                            );
                        }
                    }
                    Err(e) => {
                        error!(
                            "[proving-network] failed to prove task {:?}: {}",
                            task_key, e
                        );
                    }
                }
            }
        });

        info!("[proving-network] proving queue init end");

        handle
    }

    async fn store_proof_in_db(
        db_pool: &Arc<DbPool>,
        key: &ProvingKey,
        proof: &[u8],
    ) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT OR REPLACE INTO proofs (app_id, task_id, proof) VALUES (?, ?, ?)")
            .bind(key.app_id())
            .bind(key.task_id())
            .bind(proof)
            .execute(&**db_pool)
            .await?;
        Ok(())
    }
}
