# Pico Proving Service (CPU) - Deployment Guide

Quick deployment guide for running the pre-built Pico Proving Service CPU version.

## Prerequisites

- Docker 20.10+
- Docker Compose v2.0+
- Adequate CPU resources (recommended: 8+ cores)

Add your user to the docker group to run Docker commands without sudo (required for scripts to run properly):

```bash
# Add your current user to the docker group
sudo usermod -aG docker $USER

# Apply the new group membership without logging out
newgrp docker
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
docker load -i pico-proving-service-cpu.tar

# 2. Download gnark verification files (required for on-chain proving)
cd docker
./download-gnark.sh

# 3. Configure environment
cp env.example .env
# Edit .env with your settings (PROVER_COUNT, NUM_THREADS, etc.)

# 4. Start services
./run.sh

# Or use make
make up
```

## Service Architecture

The deployment uses a **sidecar pattern** with two containers:

- **pico-proving-service-cpu**: Main CPU proving service
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
| `PROVER_COUNT` | `32` | Number of CPU provers |
| `CHUNK_SIZE` | `2097152` | Proof chunk size |
| `CHUNK_BATCH_SIZE` | `32` | Chunk batch size |
| `SPLIT_THRESHOLD` | `1048576` | Split threshold |
| `NUM_THREADS` | `8` | CPU worker threads |
| `RUST_LOG` | `debug` | Log level |
| `VK_VERIFICATION` | `true` | Enable VK verification |

See `env.example` for all options.

## Data Persistence

- Database: `../data/pico_proving_service.db`
- Gnark files: `../gnark_downloads/`

Data persists across container restarts.

## Troubleshooting

```bash
# View detailed logs
docker logs pico-proving-service-cpu
docker logs pico-gnark-server

# Check service status
docker compose ps

# Check CPU usage
docker stats pico-proving-service-cpu
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

