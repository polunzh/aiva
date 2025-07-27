#!/bin/bash
set -e

VM_NAME="{{ vm_name }}"
DISK_GB="{{ disk_gb }}"
VM_DIR="/var/lib/firecracker/$VM_NAME"

# Create VM directory
sudo mkdir -p "$VM_DIR"
sudo chmod 755 "$VM_DIR"

# Copy and prepare rootfs
sudo cp /opt/aiva/firecracker/base.rootfs.ext4 "$VM_DIR/$VM_NAME.rootfs.ext4"
sudo chmod 644 "$VM_DIR/$VM_NAME.rootfs.ext4"

# Resize rootfs
sudo truncate -s ${DISK_GB}G "$VM_DIR/$VM_NAME.rootfs.ext4"
sudo e2fsck -f -y "$VM_DIR/$VM_NAME.rootfs.ext4" || true
sudo resize2fs "$VM_DIR/$VM_NAME.rootfs.ext4" || true

# Save configuration
cat > /tmp/config.json << 'EOF'
{{ config_json }}
EOF
sudo mv /tmp/config.json "$VM_DIR/config.json"

echo "VM created successfully"