#!/bin/bash
set -e

# Check if running in WSL2
if [ ! -f /proc/sys/fs/binfmt_misc/WSLInterop ]; then
    echo "Not running in WSL2"
    exit 1
fi

# Create directories
sudo mkdir -p /opt/aiva/firecracker /var/lib/firecracker /var/run/firecracker
sudo chmod 755 /opt/aiva/firecracker /var/lib/firecracker /var/run/firecracker

# Check if Firecracker is installed
if ! command -v firecracker >/dev/null 2>&1; then
    echo "Installing Firecracker..."
    cd /tmp
    ARCH=$(uname -m)
    if [ "$ARCH" = "x86_64" ]; then
        FC_ARCH="x86_64"
    else
        echo "Unsupported architecture: $ARCH"
        exit 1
    fi
    
    # Download Firecracker
    wget -q https://github.com/firecracker-microvm/firecracker/releases/download/v1.12.1/firecracker-v1.12.1-${FC_ARCH}.tgz
    tar -xzf firecracker-v1.12.1-${FC_ARCH}.tgz
    sudo mv release-v1.12.1-${FC_ARCH}/firecracker-v1.12.1-${FC_ARCH} /usr/local/bin/firecracker
    sudo mv release-v1.12.1-${FC_ARCH}/jailer-v1.12.1-${FC_ARCH} /usr/local/bin/jailer
    sudo chmod +x /usr/local/bin/firecracker /usr/local/bin/jailer
    rm -rf firecracker-v1.12.1-${FC_ARCH}.tgz release-v1.12.1-${FC_ARCH}
fi

# Download kernel and rootfs if needed
if [ ! -f /opt/aiva/firecracker/vmlinux ]; then
    echo "Downloading kernel..."
    sudo wget -q -O /opt/aiva/firecracker/vmlinux https://s3.amazonaws.com/spec.ccfc.min/img/quickstart_guide/x86_64/kernels/vmlinux.bin
fi

if [ ! -f /opt/aiva/firecracker/base.rootfs.ext4 ]; then
    echo "Downloading base rootfs..."
    sudo wget -q -O /opt/aiva/firecracker/base.rootfs.ext4 https://s3.amazonaws.com/spec.ccfc.min/img/quickstart_guide/x86_64/rootfs/bionic.rootfs.ext4
fi

echo "Firecracker setup complete"