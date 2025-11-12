#!/usr/bin/env bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Target directory
GNARK_DIR="${PROJECT_ROOT}/gnark_downloads/kb"

# S3 URLs
VM_CCS_URL="https://picobench.s3.us-west-2.amazonaws.com/koalabear_gnark/gpu/vm_ccs"
VM_PK_URL="https://picobench.s3.us-west-2.amazonaws.com/koalabear_gnark/gpu/vm_pk"
VM_VK_URL="https://picobench.s3.us-west-2.amazonaws.com/koalabear_gnark/gpu/vm_vk"

echo -e "${GREEN}==================================================${NC}"
echo -e "${GREEN}  Downloading gnark verification files${NC}"
echo -e "${GREEN}==================================================${NC}"
echo ""

# Check if curl or wget is available
if command -v curl &> /dev/null; then
    DOWNLOADER="curl -L -o"
elif command -v wget &> /dev/null; then
    DOWNLOADER="wget -O"
else
    echo -e "${RED}Error: Neither curl nor wget is installed${NC}"
    echo "Please install curl or wget first"
    exit 1
fi

# Create directory if it doesn't exist
echo -e "${BLUE}Creating directory: ${GNARK_DIR}${NC}"
mkdir -p "${GNARK_DIR}"

# Download function
download_file() {
    local url=$1
    local filename=$2
    local target="${GNARK_DIR}/${filename}"
    
    if [ -f "${target}" ]; then
        echo -e "${YELLOW}File ${filename} already exists, skipping...${NC}"
        return 0
    fi
    
    echo -e "${BLUE}Downloading ${filename}...${NC}"
    if command -v curl &> /dev/null; then
        curl -L -o "${target}" "${url}"
    else
        wget -O "${target}" "${url}"
    fi
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ Downloaded ${filename}${NC}"
        # Show file size
        local size=$(du -h "${target}" | cut -f1)
        echo -e "  Size: ${size}"
    else
        echo -e "${RED}✗ Failed to download ${filename}${NC}"
        return 1
    fi
    echo ""
}

# Download all files
echo -e "${BLUE}Starting downloads...${NC}"
echo ""

download_file "${VM_CCS_URL}" "vm_ccs"
download_file "${VM_PK_URL}" "vm_pk"
download_file "${VM_VK_URL}" "vm_vk"

# Verify all files exist
echo -e "${BLUE}Verifying downloads...${NC}"
all_present=true
for file in vm_ccs vm_pk vm_vk; do
    if [ -f "${GNARK_DIR}/${file}" ]; then
        echo -e "${GREEN}✓ ${file}${NC}"
    else
        echo -e "${RED}✗ ${file} missing${NC}"
        all_present=false
    fi
done
echo ""

if [ "$all_present" = true ]; then
    echo -e "${GREEN}==================================================${NC}"
    echo -e "${GREEN}  All gnark files downloaded successfully!${NC}"
    echo -e "${GREEN}==================================================${NC}"
    echo ""
    echo "Files location: ${GNARK_DIR}"
    echo ""
    echo "Total size:"
    du -sh "${GNARK_DIR}"
    echo ""
    echo "You can now build and run the Docker container."
    exit 0
else
    echo -e "${RED}==================================================${NC}"
    echo -e "${RED}  Some files are missing!${NC}"
    echo -e "${RED}==================================================${NC}"
    echo ""
    echo "Please check your internet connection and try again."
    exit 1
fi

