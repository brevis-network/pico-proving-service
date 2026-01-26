#![no_main]

pico_sdk::entrypoint!(main);

pub fn main() {
    let vk_digests: Vec<[u32; 8]> = pico_sdk::io::read_as();
    let pv_digests: Vec<[u8; 32]> = pico_sdk::io::read_as();
    assert_eq!(vk_digests.len(), pv_digests.len());

    for (vk_digest, pv_digest) in vk_digests.iter().zip(pv_digests.iter()) {
        pico_sdk::verify::verify_pico_proof(vk_digest, pv_digest);
    }
}
