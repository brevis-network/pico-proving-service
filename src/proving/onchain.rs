use crate::types::{EmbedSC, SC, Val};
use alloy_primitives::U256;
use anyhow::{Result, anyhow};
use pico_perf::common::{
    bench_field::BenchField,
    gnark_utils::{
        get_download_path, gnark_prover_running, recreate_gnark_prover, send_gnark_prove_task,
    },
};
use pico_vm::{
    instances::{
        chiptype::recursion_chiptype::RecursionChipType,
        compiler::onchain_circuit::{
            gnark::builder::OnchainVerifierCircuit, stdin::OnchainStdin,
            utils::build_gnark_config_with_str,
        },
        machine::embed::EmbedMachine,
    },
    machine::{machine::MachineBehavior, proof::MetaProof},
    primitives::consts::RECURSION_NUM_PVS,
};
use std::{path::PathBuf, sync::OnceLock, thread, time::Duration};
use tracing::info;

/// Ensure the gnark docker prover is up, generate on-chain witness JSON from the embed proof,
/// send it to the dockerized prover, and return the resulting on-chain proof bytes.
pub fn prove_embed_onchain(embed_proof: MetaProof<EmbedSC>) -> Result<Vec<u8>> {
    // 1) Ensure dockerized gnark prover is running (recreate if necessary)
    let field = BenchField::KoalaBear;
    let download_path = get_download_path(field);
    ensure_gnark_downloads(field)?;

    if !gnark_prover_running() {
        info!("[onchain] gnark prover not running, (re)creating docker container");
        recreate_gnark_prover(field, &download_path)?;
        info!("[onchain] gnark prover is ready");
    } else {
        info!("[onchain] gnark prover already running");
    }

    // 2) Build the on-chain constraints and witness, serialize to gnark witness JSON string
    let gnark_witness_json = build_onchain_witness_json(embed_proof)?;
    // std::fs::write("embed-witness.json", &gnark_witness_json).unwrap();

    // 3) Send to gnark docker server for proving
    info!("[onchain] sending witness to dockerized gnark prover");
    let proof_text = send_gnark_prove_task(gnark_witness_json)?;
    info!("[onchain] received gnark proof: {proof_text}");

    let bytes = decode_gnark_proof_to_bytes(&proof_text);
    Ok(bytes)
}

fn build_onchain_witness_json(embed_proof: MetaProof<EmbedSC>) -> Result<String> {
    // Reconstruct an EmbedMachine to obtain the base machine for OnchainStdin
    let embed_machine = EmbedMachine::<SC, EmbedSC, RecursionChipType<Val>, Vec<u8>>::new(
        EmbedSC::default(),
        RecursionChipType::<Val>::embed_chips(),
        RECURSION_NUM_PVS,
    );
    let base_machine = embed_machine.base_machine().clone();

    let vk = embed_proof
        .vks()
        .first()
        .ok_or_else(|| anyhow!("embed proof has no VKs"))?
        .clone();
    let proof = embed_proof
        .proofs()
        .first()
        .ok_or_else(|| anyhow!("embed proof has no inner proofs"))?
        .clone();

    let onchain_stdin = OnchainStdin {
        machine: base_machine,
        vk,
        proof,
        flag_complete: true,
    };

    let (constraints, witness) = OnchainVerifierCircuit::<
        pico_vm::configs::field_config::KoalaBearBn254,
        EmbedSC,
    >::build(&onchain_stdin);

    let gnark_witness = build_gnark_config_with_str(constraints, witness, &"./");
    Ok(gnark_witness)
}

fn ensure_gnark_downloads(field: BenchField) -> Result<()> {
    let dir = PathBuf::from(get_download_path(field));
    let missing_files: Vec<_> = ["vm_pk", "vm_vk", "vm_ccs"]
        .into_iter()
        .filter(|f| !dir.join(f).exists())
        .collect();
    if !missing_files.is_empty() {
        panic!(
            "[onchain] ERROR: Required gnark files are missing for {:?}. Missing files: {:?}. \
             Please ensure these files are present in the download path: {}",
            field,
            missing_files,
            dir.display()
        );
    }
    Ok(())
}

static ONCHAIN_DAEMON: OnceLock<()> = OnceLock::new();

/// Start a background daemon that monitors the dockerized gnark prover and restarts it if needed.
pub fn start_onchain_daemon() {
    if ONCHAIN_DAEMON.set(()).is_ok() {
        thread::spawn(|| {
            let field = BenchField::KoalaBear;
            let download_path = get_download_path(field);
            let interval = Duration::from_secs(15);
            loop {
                let _ = ensure_gnark_downloads(field);
                if !gnark_prover_running() {
                    let _ = recreate_gnark_prover(field, &download_path);
                }
                thread::sleep(interval);
            }
        });
        info!("[onchain] docker monitor daemon started");
    }
}

fn decode_gnark_proof_to_bytes(proof_text: &str) -> Vec<u8> {
    // remove the prefix and suffix double quotes
    let proof_text = proof_text.trim();
    let proof_text = proof_text.strip_prefix('"').unwrap_or(proof_text);
    let proof_text = proof_text.strip_suffix('"').unwrap_or(proof_text);

    // separate proof text by comma
    let values: Vec<&str> = proof_text.split(',').map(|s| s.trim()).collect();
    assert_eq!(values.len(), 10, "gnark proof must have 10 values");

    let bytes: Vec<_> = values
        .iter()
        .flat_map(|s| {
            // convert the value to an uint256
            let u256 = U256::from_str_radix(s.trim_start_matches("0x"), 16)
                .expect("failed to convert a hex string to an uint256");

            // convert the uint256 to bytes of big-endian
            u256.to_be_bytes_vec()
        })
        .collect();
    assert_eq!(
        bytes.len(),
        320,
        "ten uint256 could only be converted to 320 bytes",
    );

    // enable for debugging
    // {
    //     let values: Vec<_> = bytes
    //         .chunks(32)
    //         .map(|value| {
    //             let u = U256::from_be_slice(value);
    //             let hex_string = u
    //                 .to_be_bytes::<{ U256::BYTES }>()
    //                 .iter()
    //                 .map(|b| format!("{:02x}", b))
    //                 .collect::<String>();
    //             format!("0x{}", hex_string)
    //         })
    //         .collect();

    //     let values = values.join(",");
    //     assert_eq!(
    //         values, proof_text,
    //         "converted gnark values are not as expected",
    //     );
    // }

    bytes
}
