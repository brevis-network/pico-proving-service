#!/usr/bin/env bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

echo -e "${GREEN}==================================================${NC}"
echo -e "${GREEN}  Pico Proving Service (CPU) - Docker Build Script${NC}"
echo -e "${GREEN}==================================================${NC}"
echo ""

# Parse arguments
IMAGE_NAME="pico-proving-service-cpu"
IMAGE_TAG="latest"
NO_CACHE=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --no-cache)
            NO_CACHE="--no-cache"
            shift
            ;;
        --tag)
            IMAGE_TAG="$2"
            shift 2
            ;;
        --help)
            echo "Usage: ./build.sh [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --no-cache       Build without using cache"
            echo "  --tag TAG        Tag for the Docker image (default: latest)"
            echo "  --help           Show this help message"
            echo ""
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Check Docker is installed
if ! command -v docker &> /dev/null; then
    echo -e "${RED}Error: Docker is not installed${NC}"
    exit 1
fi

echo -e "Building image: ${GREEN}${IMAGE_NAME}:${IMAGE_TAG}${NC}"
echo -e "Project root: ${GREEN}${PROJECT_ROOT}${NC}"
echo ""

# Build the Docker image
echo -e "${GREEN}Starting Docker build...${NC}"
echo ""

cd "$PROJECT_ROOT"

docker build \
    $NO_CACHE \
    -t "${IMAGE_NAME}:${IMAGE_TAG}" \
    -f docker/cpu/Dockerfile \
    .

if [ $? -eq 0 ]; then
    echo ""
    echo -e "${GREEN}==================================================${NC}"
    echo -e "${GREEN}  Build completed successfully!${NC}"
    echo -e "${GREEN}==================================================${NC}"
    echo ""
    echo -e "Image: ${GREEN}${IMAGE_NAME}:${IMAGE_TAG}${NC}"
    echo ""
    echo "Next steps:"
    echo "  1. Copy docker/cpu/.env.example to docker/cpu/.env"
    echo "  2. Edit docker/cpu/.env with your configuration"
    echo "  3. Run: cd docker/cpu && docker-compose up -d"
    echo ""
else
    echo ""
    echo -e "${RED}==================================================${NC}"
    echo -e "${RED}  Build failed!${NC}"
    echo -e "${RED}==================================================${NC}"
    exit 1
fi

