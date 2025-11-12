# Pico proving service

## Prerequisites

- Install [Rust](https://www.rust-lang.org/tools/install): should reopen the console terminal to enable rust commands after first installation
- Install [docker](https://docs.docker.com/engine/install) and add current user to docker group `sudo groupadd docker 2>/dev/null || true && sudo usermod -aG docker $USER`
- Install Development Tools: `sudo apt-get install -y build-essential cmake git pkg-config libssl-dev`
- Install [protoc](https://github.com/brevis-network/pico-ethproofs/blob/main/docs/multi-machine-setup.md#install-protoc)
- Install [sqlx-cli](https://github.com/launchbadge/sqlx/tree/main/sqlx-cli) (`cargo install sqlx-cli`)

## Machine and OS

- AWS: `r7i.16xlarge` (64 CPUs)
- OS: `ubuntu-24.04-amd64-server`

## Local DB initialization

Use the installed `sqlx` command to create the sqlite database and run the migrations:
```
# clone and enter pico-proving-service repository
cd pico-proving-service

# create database
sqlx database create

# run migrations
sqlx migrate run
```

## Service start

Start the service:
```
# unlimit the handlers
ulimit -s unlimited

# GRPC service address is bound to `0.0.0.0:50052` as default, it could be set by `GRPC_ADDR` ENV
# export GRPC_ADDR="0.0.0.0:50052"

# enable debug log and backtrace
export RUST_LOG=debug
export RUST_BACKTRACE=full

# set performance related config
export RUSTFLAGS="-C target-cpu=native -C target-feature=+avx512f,+avx512ifma,+avx512vl"
export JEMALLOC_SYS_WITH_MALLOC_CONF="retain:true,background_thread:true,metadata_thp:always,dirty_decay_ms:-1,muzzy_decay_ms:-1,abort_conf:true"

# chunk size settings
export CHUNK_SIZE=2097152
export CHUNK_BATCH_SIZE=32
export SPLIT_THRESHOLD=1048576

# set emulator thread number
export NUM_THREADS=8

# set prover number
export PROVER_COUNT=32
export RUST_MIN_STACK=16777216

# Set VK_VERIFICATION=true to select the predetermined recursion circuit.
# This environment variable restricts recursion verification to a fixed, predefined set of circuits.
export VK_VERIFICATION=true

# set the maximum supported emulation cycles
# export MAX_EMULATION_CYCLES=200000000 # 200M

cargo run -r --bin server
```

## GRPC API

### Common result and errors

```
message ErrMsg {
    ErrCode code = 1;
    string msg = 2;
}

enum ErrCode {
    OK = 0;
    // invalid arguments
    INVAL = 1;
    // internal error
    INTERNAL = 2;
    // proving in-progress (deprecated)
    PROVING_PENDING = 3;
    // proving failed
    PROVING_FAILED = 4; (deprecated)
    // input exceeds supported maximum emulation cycles
    INPUT_EXCEEDED = 5;
}
```

### Register application

Register a new application or update an existing one (e.g. ELF or program information).
The further cost estimating or proving requests use `app_id` for interactions.
```
service ProverNetwork {
  rpc RegisterApp(RegisterAppRequest) returns(RegisterAppResponse);
}

message RegisterAppRequest {
  // program elf data
  bytes elf = 1;
  // optional program information
  optional string info = 2;
}

message RegisterAppResponse {
  // common result
  ErrMsg err = 1;
  // application hash
  string app_id = 2;
}
```

### Estimate cost

```
service ProverNetwork {
  rpc EstimateCost(EstimateCostRequest) returns(EstimateCostResponse);
}

message EstimateCostRequest {
  // application hash
  string app_id = 1;
  // input array, empty if no inputs
  repeated bytes inputs = 2;
}

message EstimateCostResponse {
  // common result
  ErrMsg err = 1;
  // gas cost
  uint64 cost = 2;
  // public values digest
  bytes pv_digest = 3;
}
```

### Prove with input

The proving API is asynchronous, the result should be fetched in another API.
```
service ProverNetwork {
  rpc ProveTask(ProveTaskRequest) returns(ProveTaskResponse);
}

message ProveTaskRequest {
  // application hash
  string app_id = 1;
  // proving task ID, it should be unique for this application
  string task_id = 2;
  // input array, empty if no inputs
  repeated bytes inputs = 3;
}

message ProveTaskResponse {
  // common result
  ErrMsg err = 1;
}
```

### Get proving result

This API could return `PROVING_PENDING` if proving in-progress, and `PROVING_FAILED` if failed.
```
service ProverNetwork {
  rpc GetProvingResult(GetProvingResultRequest) returns(GetProvingResultResponse);
}

message GetProvingResultRequest {
  // application hash
  string app_id = 1;
  // proving task ID, it should be unique for this application
  string task_id = 2;
}

message GetProvingResultResponse {
  // common result
  ErrMsg err = 1;
  // groth16 proof, it's valid if the result code is `OK`
  optional bytes proof = 2;
}
```

## Test CLI

### Generate application ID locally

```
RUST_LOG=debug VK_VERIFICATION=true cargo run -r --bin gen-app-id -- --elf ./fixtures/reth-elf
```

### Generate reth inputs and public values digest

This command generates the reth inputs and public values digest, and saves them into files as
`reth_input_BLOCK_NUMBER.bin` and `reth_pv_digest_BLOCK_NUMBER.bin`.
```
# set emulator thread number
export NUM_THREADS=8

RUST_LOG=debug VK_VERIFICATION=true cargo run -r --bin gen-reth-inputs -- --block-number LATEST_BLOCK_NUMBER --rpc-url DEBUG_RPC_URL
```

### Generate common public values digest locally

```
RUST_LOG=debug VK_VERIFICATION=true cargo run -r --bin gen-common-pv-digest -- --elf ./fixtures/reth-elf --inputs ./fixtures/reth-18884864.bin
```

### Register application

```
RUST_LOG=debug VK_VERIFICATION=true cargo run -r --bin test-client register-app --elf ./fixtures/reth-elf
```

### Estimate cost

```
RUST_LOG=debug cargo run -r --bin test-client estimate-cost --app-id APP_ID --inputs ./fixtures/reth-18884864.bin
```

### Prove with input

```
RUST_LOG=debug cargo run -r --bin test-client prove-task --app-id APP_ID --task-id reth-188 --inputs ./fixtures/reth-18884864.bin
```

### Get proving result

```
RUST_LOG=debug cargo run -r --bin test-client get-proving-result --app-id APP_ID --task-id reth-188
```

## Test on-chain

The Groth16 Verifier contract has been deployed on Sepolia. It can be used for testing the final proof verification.  

[View contract on Sepolia](https://sepolia.arbiscan.io/address/0xf35253Ec37A139243DdC74be32B9da868C4fa576#readContract)
