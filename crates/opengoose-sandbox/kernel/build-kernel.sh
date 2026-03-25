#!/bin/bash
# Build minimal ARM64 Linux kernel for opengoose-sandbox.
# Requires: aarch64-linux-musl-gcc (brew install filosottile/musl-cross/musl-cross)
set -euo pipefail

KERNEL_VERSION="6.12.77"
KERNEL_URL="https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-${KERNEL_VERSION}.tar.xz"
BUILD_DIR="/tmp/opengoose-kernel-build"
OUTPUT_DIR="${HOME}/.opengoose/kernel/aarch64"

mkdir -p "${BUILD_DIR}" "${OUTPUT_DIR}"

# Download kernel source if not cached
if [ ! -d "${BUILD_DIR}/linux-${KERNEL_VERSION}" ]; then
    echo "Downloading Linux ${KERNEL_VERSION}..."
    curl -L "${KERNEL_URL}" | tar -xJ -C "${BUILD_DIR}"
fi

cd "${BUILD_DIR}/linux-${KERNEL_VERSION}"

# Start from defconfig (has sane defaults + dependency chains resolved)
gmake ARCH=arm64 CROSS_COMPILE=aarch64-linux-musl- defconfig

# Enable what we need (built-in, not module)
./scripts/config --enable CONFIG_VIRTIO
./scripts/config --enable CONFIG_VIRTIO_MMIO
./scripts/config --set-val CONFIG_VIRTIO_MMIO y
./scripts/config --enable CONFIG_VIRTIO_CONSOLE
./scripts/config --set-val CONFIG_VIRTIO_CONSOLE y
./scripts/config --enable CONFIG_HVC_DRIVER
./scripts/config --enable CONFIG_SERIAL_AMBA_PL011
./scripts/config --enable CONFIG_SERIAL_AMBA_PL011_CONSOLE
./scripts/config --enable CONFIG_SERIAL_EARLYCON
./scripts/config --enable CONFIG_ARM_GIC_V3
./scripts/config --enable CONFIG_BLK_DEV_INITRD
./scripts/config --enable CONFIG_DEVTMPFS
./scripts/config --enable CONFIG_DEVTMPFS_MOUNT
./scripts/config --enable CONFIG_PROC_FS
./scripts/config --enable CONFIG_SYSFS
./scripts/config --enable CONFIG_TMPFS

# Disable stuff we don't need (reduce size + build time)
./scripts/config --disable CONFIG_MODULES
./scripts/config --disable CONFIG_NETWORK
./scripts/config --disable CONFIG_NET
./scripts/config --disable CONFIG_WIRELESS
./scripts/config --disable CONFIG_WLAN
./scripts/config --disable CONFIG_BT
./scripts/config --disable CONFIG_SOUND
./scripts/config --disable CONFIG_USB_SUPPORT
./scripts/config --disable CONFIG_DRM
./scripts/config --disable CONFIG_INPUT
./scripts/config --disable CONFIG_HID
./scripts/config --disable CONFIG_SWAP
./scripts/config --disable CONFIG_SUSPEND
./scripts/config --disable CONFIG_HIBERNATION
./scripts/config --disable CONFIG_PM
./scripts/config --disable CONFIG_CRYPTO
./scripts/config --disable CONFIG_SECURITY
./scripts/config --disable CONFIG_AUDIT
./scripts/config --disable CONFIG_PERF_EVENTS
./scripts/config --disable CONFIG_FTRACE
./scripts/config --disable CONFIG_DEBUG_KERNEL
./scripts/config --disable CONFIG_STRICT_DEVMEM
./scripts/config --disable CONFIG_IO_STRICT_DEVMEM
./scripts/config --disable CONFIG_PROFILING
./scripts/config --disable CONFIG_KPROBES

# Resolve dependencies
gmake ARCH=arm64 CROSS_COMPILE=aarch64-linux-musl- olddefconfig

# Verify critical configs
echo "=== Verifying config ==="
for cfg in VIRTIO VIRTIO_MMIO VIRTIO_CONSOLE SERIAL_AMBA_PL011 BLK_DEV_INITRD DEVTMPFS; do
    val=$(grep "CONFIG_${cfg}=" .config || echo "NOT SET")
    echo "  ${cfg}: ${val}"
done

echo "Building kernel (this takes a few minutes)..."
NCPU=$(sysctl -n hw.ncpu 2>/dev/null || nproc 2>/dev/null || echo 4)
gmake ARCH=arm64 CROSS_COMPILE=aarch64-linux-musl- -j"${NCPU}" Image 2>&1 | tail -5

cp arch/arm64/boot/Image "${OUTPUT_DIR}/Image.custom"
echo "Kernel built: ${OUTPUT_DIR}/Image.custom ($(du -h "${OUTPUT_DIR}/Image.custom" | cut -f1))"
