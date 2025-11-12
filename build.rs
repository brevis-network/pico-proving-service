fn main() {
    tonic_build::compile_protos("proto/prover_network.proto").unwrap();
    tonic_build::compile_protos("proto/proving.proto").unwrap();
}
