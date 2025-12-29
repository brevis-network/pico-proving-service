use anyhow::{Result, anyhow};
use reqwest::blocking::Client;
use std::{
    env,
    time::{Duration, Instant},
};
use tracing::info;

/// Check if we're running in Docker mode (sidecar pattern)
pub fn is_sidecar_mode() -> bool {
    env::var("GNARK_SIDECAR_MODE")
        .unwrap_or_else(|_| "false".to_string())
        .to_lowercase()
        == "true"
}

/// Get the gnark service URL (either sidecar or localhost)
pub fn get_gnark_url() -> String {
    env::var("GNARK_URL").unwrap_or_else(|_| "http://127.0.0.1:9099".to_string())
}

/// Check if gnark prover is running (sidecar version)
pub fn gnark_prover_running_sidecar() -> bool {
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .ok();

    if let Some(client) = client {
        let url = format!("{}/ready", get_gnark_url());
        if let Ok(response) = client
            .post(&url)
            .header("Content-Type", "application/json")
            .send()
        {
            return response.status().is_success();
        }
    }
    false
}

/// Wait for gnark prover to be ready (sidecar version)
pub fn wait_for_gnark_ready() -> Result<()> {
    let client = Client::new();
    let start = Instant::now();
    let timeout = Duration::from_secs(120);
    let poll_interval = Duration::from_secs(2);
    let url = format!("{}/ready", get_gnark_url());

    info!(
        "[onchain] waiting for gnark prover to be ready at {}",
        get_gnark_url()
    );

    loop {
        match client
            .post(&url)
            .header("Content-Type", "application/json")
            .timeout(Duration::from_secs(2))
            .send()
        {
            Ok(resp) if resp.status().is_success() => {
                info!("[onchain] gnark prover is ready");
                return Ok(());
            }
            Ok(resp) => {
                info!(
                    "[onchain] gnark prover not ready (status: {}), waiting...",
                    resp.status()
                );
            }
            Err(e) => {
                info!("[onchain] gnark prover not reachable ({}), waiting...", e);
            }
        }

        if start.elapsed() > timeout {
            return Err(anyhow!("timeout waiting for gnark prover to be ready"));
        }
        std::thread::sleep(poll_interval);
    }
}

/// Send proof task to gnark (works with both sidecar and local)
pub fn send_gnark_prove_task_sidecar(json_req: String) -> Result<String> {
    let client = Client::new();
    let url = format!("{}/prove", get_gnark_url());

    info!("[onchain] sending witness to gnark prover at {}", url);

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(json_req)
        .send()?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Failed to prove task: {} {}",
            response.status(),
            response.text()?
        ));
    }

    info!("[onchain] gnark prover successful");
    response.text().map_err(Into::into)
}
