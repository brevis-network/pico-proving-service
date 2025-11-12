use super::config::ServiceConfig;
use crate::{
    EstimateCostRequest, EstimateCostResponse, GetProvingResultRequest, GetProvingResultResponse,
    ProveTaskRequest, ProveTaskResponse, RegisterAppRequest, RegisterAppResponse,
    app_manager::AppManager,
    cost_estimation::estimate_cost,
    prover_network_server::{ProverNetwork, ProverNetworkServer},
    proving_queue::{ProvingKey, ProvingOutputs, ProvingTask},
    types::DbPool,
    utils::auth::AuthConfig,
};
use anyhow::Result;
use crossbeam::channel::Sender;
use std::sync::Arc;
use tokio::{signal::ctrl_c, task::JoinHandle};
use tonic::{
    Request, Response, Status, async_trait,
    codec::CompressionEncoding,
    service::{LayerExt, interceptor::InterceptedService},
    transport::Server,
};
use tonic_web::GrpcWebLayer;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

pub struct GrpcService {
    cfg: ServiceConfig,
    app_manager: AppManager,
    db_pool: Arc<DbPool>,
    outputs: Arc<ProvingOutputs>,
    sender: Arc<Sender<ProvingTask>>,
}

impl GrpcService {
    pub fn new(
        cfg: ServiceConfig,
        db_pool: Arc<DbPool>,
        outputs: Arc<ProvingOutputs>,
        sender: Arc<Sender<ProvingTask>>,
    ) -> Self {
        let app_manager = AppManager::new(db_pool.clone());

        Self {
            cfg,
            app_manager,
            db_pool,
            outputs,
            sender,
        }
    }

    pub fn run(self) -> JoinHandle<()> {
        info!("[proving-network] grpc server init");
        let handle = tokio::spawn(async move {
            let cfg = &self.cfg;
            let addr = cfg.grpc_addr;
            let max_grpc_msg_size = cfg.max_grpc_msg_size;
            let auth_interceptor = cfg.server_auth_interceptor();

            let base = InterceptedService::new(
                ProverNetworkServer::new(self)
                    .max_encoding_message_size(max_grpc_msg_size)
                    .max_decoding_message_size(max_grpc_msg_size)
                    .accept_compressed(CompressionEncoding::Zstd)
                    .send_compressed(CompressionEncoding::Zstd),
                auth_interceptor,
            );

            let svc = ServiceBuilder::new()
                .layer(
                    CorsLayer::new()
                        .allow_origin(Any)
                        .allow_methods(Any)
                        .allow_headers(Any),
                )
                .layer(GrpcWebLayer::new())
                .into_inner()
                .named_layer(base);

            Server::builder()
                .accept_http1(true)
                .add_service(svc)
                .serve_with_shutdown(addr, async {
                    ctrl_c().await.expect("failed to wait for shutdown");
                })
                .await
                .expect("failed");
        });

        info!("[proving-network] grpc server init end");

        handle
    }
}

#[async_trait]
impl ProverNetwork for GrpcService {
    // register a new application with elf
    async fn register_app(
        &self,
        req: Request<RegisterAppRequest>,
    ) -> Result<Response<RegisterAppResponse>, Status> {
        info!("receive RegisterAppRequest");

        let req = req.into_inner();
        let app = self
            .app_manager
            .set_app(&req.elf, req.info)
            .await
            .map_err(|e| Status::internal(format!("failed to register app: {e}")))?;
        let app_id = app.app_id;

        info!("return RegisterAppResponse");

        Ok(Response::new(RegisterAppResponse { err: None, app_id }))
    }

    // estimate gas cost
    async fn estimate_cost(
        &self,
        req: Request<EstimateCostRequest>,
    ) -> Result<Response<EstimateCostResponse>, Status> {
        info!("receive EstimateCostRequest");

        let req = req.into_inner();
        let app_id = req.app_id;
        let app = self
            .app_manager
            .get_app(&app_id)
            .await
            .map_err(|e| Status::internal(format!("failed to get app: {e}")))?
            .ok_or_else(|| Status::not_found(format!("cannot find app {app_id}")))?;

        let res = match estimate_cost(
            app.program,
            app.pk,
            app.vk,
            req.inputs.as_deref(),
            self.cfg.max_emulation_cycles,
            true,
        ) {
            Ok(info) => EstimateCostResponse {
                err: None,
                cost: info.cost,
                pv_digest: info.pv_digest.to_be_bytes_vec(),
            },
            Err(e) => e.into(),
        };

        info!("return EstimateCostResponse");

        Ok(Response::new(res))
    }

    // add a proving task
    async fn prove_task(
        &self,
        req: Request<ProveTaskRequest>,
    ) -> Result<Response<ProveTaskResponse>, Status> {
        info!("receive ProveTaskRequest");

        let req = req.into_inner();
        let app_id = req.app_id;
        let app = self
            .app_manager
            .get_app(&app_id)
            .await
            .map_err(|e| Status::internal(format!("failed to get app: {e}")))?
            .ok_or_else(|| Status::not_found(format!("cannot find app {app_id}")))?;

        let key = ProvingKey::new(app_id, req.task_id);
        // Default to cpu if not specified
        let use_gpu = req.use_gpu.unwrap_or(false);
        let task = ProvingTask::new(
            key,
            app.program,
            Arc::new(app.pk),
            Arc::new(app.vk),
            req.inputs,
            use_gpu,
        );
        self.sender
            .send(task)
            .map_err(|e| Status::internal(format!("failed to send a proving task: {e}")))?;

        info!("return ProveTaskResponse");

        Ok(Response::new(ProveTaskResponse { err: None }))
    }

    // try to fetch the proving result if complete
    async fn get_proving_result(
        &self,
        req: Request<GetProvingResultRequest>,
    ) -> Result<Response<GetProvingResultResponse>, Status> {
        info!("receive GetProvingResultRequest");

        let req = req.into_inner();
        let key = ProvingKey::new(req.app_id, req.task_id);

        info!("[grpc] looking for proof with key: {:?}", key);

        // First try to get from memory (for recently completed proofs)
        info!("[grpc] checking memory for key: {:?}", key);
        info!("[grpc] current memory entries: {}", self.outputs.len());

        let proof = if let Some((_, output)) = self.outputs.remove(&key) {
            info!(
                "[grpc] found proof in memory, size: {} bytes",
                output.proof.len()
            );
            Some(output.proof)
        } else {
            info!("[grpc] proof not in memory, checking database");
            // If not in memory, try to get from database
            let db_proof = sqlx::query_as::<_, (Option<Vec<u8>>,)>(
                "SELECT proof FROM proofs WHERE app_id = ? AND task_id = ?",
            )
            .bind(&key.app_id())
            .bind(&key.task_id())
            .fetch_optional(&*self.db_pool)
            .await
            .map_err(|e| Status::internal(format!("failed to get proof from database: {e}")))?
            .and_then(|row| row.0);

            if let Some(ref proof_data) = db_proof {
                info!(
                    "[grpc] found proof in database, size: {} bytes",
                    proof_data.len()
                );
            } else {
                info!("[grpc] proof not found in database");
            }

            db_proof.map(Arc::from)
        };

        info!("return GetProvingResultResponse");

        Ok(Response::new(GetProvingResultResponse {
            err: None,
            proof: proof.map(|arc_proof| arc_proof.to_vec()),
        }))
    }
}
