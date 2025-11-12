#!/usr/bin/env bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${GREEN}==================================================${NC}"
echo -e "${GREEN}  Pico Proving Service (CPU) - Docker Deployment${NC}"
echo -e "${GREEN}==================================================${NC}"
echo ""

# Check prerequisites
if ! command -v docker &> /dev/null; then
    echo -e "${RED}Error: Docker is not installed${NC}"
    exit 1
fi

if ! command -v docker-compose &> /dev/null && ! docker compose version &> /dev/null 2>&1; then
    echo -e "${RED}Error: Docker Compose is not installed${NC}"
    exit 1
fi

# Determine docker compose command
if command -v docker-compose &> /dev/null; then
    COMPOSE_CMD="docker-compose"
else
    COMPOSE_CMD="docker compose"
fi

# Setup environment
if [ ! -f .env ]; then
    echo -e "${YELLOW}No .env file found. Creating from template...${NC}"
    if [ -f env.example ]; then
        cp env.example .env
        echo -e "${GREEN}Created .env file from template${NC}"
        echo -e "${YELLOW}Please review and edit .env before starting the service${NC}"
        echo ""
        read -p "Press Enter to continue after editing .env..."
    else
        echo -e "${RED}Error: env.example not found${NC}"
        exit 1
    fi
fi

# Check gnark files
if [ ! -d "../gnark_downloads/kb" ] || [ ! -f "../gnark_downloads/kb/vm_pk" ]; then
    echo -e "${YELLOW}gnark verification files not found${NC}"
    echo -e "${YELLOW}These files are required for on-chain proving${NC}"
    echo ""
    read -p "Download gnark files now? (y/N) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        ./download-gnark.sh
        if [ $? -ne 0 ]; then
            echo -e "${RED}Failed to download gnark files${NC}"
            exit 1
        fi
    else
        echo -e "${YELLOW}Warning: Continuing without gnark files${NC}"
        echo -e "${YELLOW}On-chain proving will not work${NC}"
        echo ""
    fi
fi

# Check if server image exists
if ! docker image inspect pico-proving-service-cpu:latest &> /dev/null; then
    echo -e "${RED}Error: Docker image 'pico-proving-service-cpu:latest' not found${NC}"
    echo -e "${YELLOW}Please load the Docker image first:${NC}"
    echo -e "${BLUE}  docker load -i pico-proving-service-cpu.tar${NC}"
    exit 1
fi

# Create data directory
echo -e "${BLUE}Creating data directory...${NC}"
mkdir -p ../data
echo -e "${GREEN}Data directory ready at: $(cd .. && pwd)/data${NC}"
echo ""

# Start the service
echo -e "${GREEN}Starting Pico Proving Service (CPU)...${NC}"
$COMPOSE_CMD up -d

if [ $? -eq 0 ]; then
    echo ""
    echo -e "${GREEN}==================================================${NC}"
    echo -e "${GREEN}  Service started successfully!${NC}"
    echo -e "${GREEN}==================================================${NC}"
    echo ""
    echo "Services running:"
    echo "  - pico-proving-service-cpu (CPU proving)"
    echo "  - pico-gnark-server (on-chain proofs)"
    echo ""
    echo "View logs:"
    echo "  ${BLUE}$COMPOSE_CMD logs -f${NC}"
    echo ""
    echo "Check status:"
    echo "  ${BLUE}$COMPOSE_CMD ps${NC}"
    echo ""
    echo "Stop service:"
    echo "  ${BLUE}$COMPOSE_CMD down${NC}"
    echo ""
    echo "Access server container:"
    echo "  ${BLUE}docker exec -it pico-proving-service-cpu bash${NC}"
    echo ""
else
    echo ""
    echo -e "${RED}==================================================${NC}"
    echo -e "${RED}  Failed to start service${NC}"
    echo -e "${RED}==================================================${NC}"
    echo ""
    echo "Check logs for errors:"
    echo "  ${BLUE}$COMPOSE_CMD logs${NC}"
    exit 1
fi
