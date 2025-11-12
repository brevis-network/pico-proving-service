# Pico Proving Service - Deployment Guide

Quick deployment guide for running the pre-built Pico Proving Service.

## Prerequisites

- Docker 20.10+
- Docker Compose v2.0+
- NVIDIA GPU with CUDA support (RTX 3090/4090/5090)
- NVIDIA Container Toolkit

Add your user to the docker group to run Docker commands without sudo (required for scripts to run properly):

```bash
# Add your current user to the docker group
sudo usermod -aG docker $USER

# Apply the new group membership without logging out
newgrp docker
```

Ensure your system supports GPU passthrough with Docker. If you encounter this error:

```bash
could not select device driver "" with capabilities: [[gpu]]
```

You need to install the [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html), then restart Docker:

```bash
sudo systemctl restart docker
```

## Files Included

- `docker-compose.yml` - Service orchestration
- `Makefile` - Convenient commands
- `run.sh` - Quick start script
- `env.example` - Configuration template
- `download-gnark.sh` - Download gnark verification files

## Quick Start

```bash
# 1. Load the Docker image
docker load -i pico-proving-service-gpu.tar

# 2. Download gnark verification files (required for on-chain proving)
cd docker
./download-gnark.sh

# 3. Configure environment
cp env.example .env
# Edit .env with your settings (PROVER_COUNT, GRPC_PORT, etc.)

# 4. Start services
./run.sh

# Or use make
make up
```

## Service Architecture

The deployment uses a **sidecar pattern** with two containers:

- **pico-proving-service-gpu**: Main GPU proving service
- **pico-gnark-server**: On-chain proof generation service

Both services communicate over a private Docker network. The gnark service is automatically configured and managed.

## Common Commands

```bash
# View logs
make logs              # All services
make logs-server       # Server only
make logs-gnark        # Gnark only

# Service management
make status            # Show status
make restart           # Restart
make down              # Stop all services

# Access container
make exec              # Open shell in server container
```

## Configuration

Edit `.env` to customize:

| Variable | Default | Description |
|----------|---------|-------------|
| `GRPC_ADDR` | `0.0.0.0:50052` | gRPC listen address |
| `PROVER_COUNT` | `1` | Number of GPU provers |
| `CHUNK_SIZE` | `2097152` | Proof chunk size |
| `NUM_THREADS` | `6` | CPU worker threads |
| `RUST_LOG` | `info` | Log level |

See `env.example` for all options.

## Data Persistence

- Database: `../data/pico_proving_service.db`
- Gnark files: `../gnark_downloads/`

Data persists across container restarts.

## Troubleshooting

```bash
# Check GPU access
docker run --rm --gpus all nvidia/cuda:12.8.1-runtime-ubuntu22.04 nvidia-smi

# View detailed logs
docker logs pico-proving-service-gpu
docker logs pico-gnark-server

# Check service status
docker compose ps
```

## Stopping the Service

```bash
# Stop all services
make down

# Or use run.sh equivalent
docker compose down
```

## Port Configuration

Default ports:
- `50052` - gRPC API (server)
- `9099` - Gnark service (internal only)

Modify in `.env` if needed:
```bash
GRPC_PORT=50052
```

