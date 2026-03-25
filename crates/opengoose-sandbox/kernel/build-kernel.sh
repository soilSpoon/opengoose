#!/bin/bash
# Build minimal ARM64 Linux kernel for opengoose-sandbox.
# Requires: Docker (build-time only)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
OUTPUT_DIR="${HOME}/.opengoose/kernel/aarch64"
IMAGE_NAME="opengoose-kernel-builder"

mkdir -p "${OUTPUT_DIR}"

echo "Building kernel via Docker (takes a few minutes on first run)..."
docker build --platform linux/arm64 -t "${IMAGE_NAME}" "${SCRIPT_DIR}"

# Extract the Image from the build container
CONTAINER=$(docker create "${IMAGE_NAME}")
docker cp "${CONTAINER}:/linux-6.12.77/arch/arm64/boot/Image" "${OUTPUT_DIR}/Image.custom"
docker rm "${CONTAINER}" > /dev/null

SIZE=$(du -h "${OUTPUT_DIR}/Image.custom" | cut -f1)
echo "Done: ${OUTPUT_DIR}/Image.custom (${SIZE})"
