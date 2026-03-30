FROM --platform=linux/arm64 ubuntu:22.04 AS builder
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential bc flex bison libssl-dev libelf-dev wget xz-utils ca-certificates \
    && rm -rf /var/lib/apt/lists/*

ARG KERNEL_VERSION=6.12.77
RUN wget -q https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-${KERNEL_VERSION}.tar.xz \
    && tar xf linux-${KERNEL_VERSION}.tar.xz \
    && rm linux-${KERNEL_VERSION}.tar.xz

WORKDIR /linux-${KERNEL_VERSION}

# Start from tinyconfig (absolute minimum), then add only what we need
RUN make ARCH=arm64 tinyconfig

# Core requirements for our VM
RUN ./scripts/config \
    --enable CONFIG_PRINTK \
    --enable CONFIG_BUG \
    --enable CONFIG_OF \
    --enable CONFIG_OF_EARLY_FLATTREE \
    --enable CONFIG_SERIAL_EARLYCON \
    --enable CONFIG_SERIAL_AMBA_PL011 \
    --enable CONFIG_SERIAL_AMBA_PL011_CONSOLE \
    --enable CONFIG_ARM_GIC_V3 \
    --enable CONFIG_ARM_ARCH_TIMER \
    --enable CONFIG_GENERIC_CLOCKEVENTS \
    --enable CONFIG_HZ_250 \
    --enable CONFIG_VIRTIO \
    --enable CONFIG_VIRTIO_MENU \
    --enable CONFIG_VIRTIO_MMIO \
    --enable CONFIG_VIRTIO_CONSOLE \
    --enable CONFIG_HVC_DRIVER \
    --enable CONFIG_FUSE_FS \
    --enable CONFIG_VIRTIO_FS \
    --enable CONFIG_OVERLAY_FS \
    --enable CONFIG_BLK_DEV_INITRD \
    --enable CONFIG_BINFMT_ELF \
    --enable CONFIG_DEVTMPFS \
    --enable CONFIG_DEVTMPFS_MOUNT \
    --enable CONFIG_PROC_FS \
    --enable CONFIG_SYSFS \
    --enable CONFIG_TMPFS \
    --enable CONFIG_TTY \
    --enable CONFIG_VT \
    --enable CONFIG_UNIX98_PTYS \
    --enable CONFIG_MULTIUSER \
    --enable CONFIG_FUTEX \
    --enable CONFIG_EPOLL \
    --enable CONFIG_SIGNALFD \
    --enable CONFIG_TIMERFD \
    --enable CONFIG_EVENTFD \
    --enable CONFIG_ADVISE_SYSCALLS \
    --disable CONFIG_MODULES \
    --disable CONFIG_NET \
    --disable CONFIG_STRICT_DEVMEM \
    --disable CONFIG_IO_STRICT_DEVMEM \
    --disable CONFIG_DEBUG_KERNEL

# Resolve dependencies and build
RUN make ARCH=arm64 olddefconfig
RUN make ARCH=arm64 -j$(nproc) Image

# Verify critical configs — fail the build if any required symbol is missing
RUN grep -Eq '^CONFIG_VIRTIO_CONSOLE=y$' .config \
    && grep -Eq '^CONFIG_VIRTIO_MMIO=y$' .config \
    && grep -Eq '^CONFIG_VIRTIO_FS=y$' .config \
    && grep -Eq '^CONFIG_FUSE_FS=y$' .config \
    && grep -Eq '^CONFIG_OVERLAY_FS=y$' .config \
    && grep -Eq '^CONFIG_SERIAL_AMBA_PL011=y$' .config \
    && grep -Eq '^# CONFIG_MODULES is not set$' .config
