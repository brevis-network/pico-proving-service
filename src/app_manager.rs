use crate::types::{DbPool, SC, Val};
use anyhow::{Result, bail};
use derive_more::Constructor;
use pico_vm::{
    compiler::riscv::{
        compiler::{Compiler, SourceType},
        program::Program,
    },
    instances::{
        chiptype::riscv_chiptype::RiscvChipType,
        compiler::{shapes::riscv_shape::RiscvShapeConfig, vk_merkle::vk_verification_enabled},
        machine::riscv::RiscvMachine,
    },
    machine::{
        keys::{BaseProvingKey, BaseVerifyingKey, HashableKey},
        machine::MachineBehavior,
    },
    primitives::consts::RISCV_NUM_PVS,
};
use sqlx::FromRow;
use std::sync::Arc;
use tracing::info;

#[derive(Clone)]
pub struct App {
    pub app_id: String,
    pub program: Arc<Program>,
    pub pk: BaseProvingKey<SC>,
    pub vk: BaseVerifyingKey<SC>,
    pub info: Option<String>,
}

impl App {
    // create an app
    pub fn new(elf: &[u8], info: Option<String>) -> Self {
        info!("compiling elf to program");
        let mut program = Compiler::new(SourceType::RISCV, elf).compile();

        if vk_verification_enabled() {
            info!("padding shape");
            let shape_config = RiscvShapeConfig::<Val>::default();
            let p = Arc::get_mut(&mut program).expect("cannot get program");
            shape_config
                .padding_preprocessed_shape(p)
                .expect("cannot padding preprocessed shape");
        }

        info!("creating riscv machine");
        let machine = RiscvMachine::new(SC::default(), RiscvChipType::all_chips(), RISCV_NUM_PVS);

        info!("setting up pk and vk");
        let (pk, vk) = machine.setup_keys(&program);

        let app_id = vk.hash_str_via_bn254();
        assert_eq!(
            app_id.len(),
            66,
            "app-id must be an uint256 starting with 0x",
        );
        let app_id = app_id[2..].to_string();

        Self {
            app_id,
            program,
            pk,
            vk,
            info,
        }
    }
}

#[derive(Debug, FromRow)]
pub struct AppRow {
    pub app_id: String,
    pub program: Vec<u8>,
    pub pk: Vec<u8>,
    pub vk: Vec<u8>,
    pub info: Option<String>,
}

impl From<App> for AppRow {
    fn from(app: App) -> Self {
        let app_id = app.app_id;
        let program = bincode::serialize(&app.program).unwrap();
        let pk = bincode::serialize(&app.pk).unwrap();
        let vk = bincode::serialize(&app.vk).unwrap();
        let info = app.info;

        Self {
            app_id,
            program,
            pk,
            vk,
            info,
        }
    }
}

impl From<AppRow> for App {
    fn from(row: AppRow) -> Self {
        let app_id = row.app_id;
        let program = Arc::new(bincode::deserialize(&row.program).unwrap());
        let pk = bincode::deserialize(&row.pk).unwrap();
        let vk = bincode::deserialize(&row.vk).unwrap();
        let info = row.info;

        Self {
            app_id,
            program,
            pk,
            vk,
            info,
        }
    }
}

#[derive(Constructor)]
pub struct AppManager {
    db_pool: Arc<DbPool>,
}

impl AppManager {
    pub async fn get_app(&self, app_id: &str) -> Result<Option<App>> {
        // remove the prefix `0x`
        let app_id = app_id.strip_prefix("0x").unwrap_or(app_id);

        let row = sqlx::query_as::<_, AppRow>(
            "SELECT app_id, program, pk, vk, info FROM apps WHERE app_id = ?",
        )
        .bind(app_id)
        .fetch_optional(&*self.db_pool)
        .await?;

        Ok(row.map(Into::into))
    }

    pub async fn set_app(&self, elf: &[u8], info: Option<String>) -> Result<App> {
        let app = App::new(elf, info);

        let app_id = &app.app_id;
        info!("register an new app {app_id}");

        if self.get_app(app_id).await?.is_some() {
            bail!("app already exists {app_id}");
        }

        let row = AppRow::from(app.clone());

        info!("saving app to DB");
        sqlx::query("INSERT INTO apps (app_id, program, pk, vk, info) VALUES (?, ?, ?, ?, ?)")
            .bind(&row.app_id)
            .bind(&row.program)
            .bind(&row.pk)
            .bind(&row.vk)
            .bind(&row.info)
            .execute(&*self.db_pool)
            .await?;

        Ok(app)
    }
}
