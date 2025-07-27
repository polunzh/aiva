#!/bin/bash
VM_NAME="{{ vm_name }}"
VM_DIR="/var/lib/firecracker/$VM_NAME"

# Remove VM directory and all files
sudo rm -rf "$VM_DIR"

# Clean up any remaining resources
sudo ip link delete tap-$VM_NAME 2>/dev/null || true

echo "VM deleted successfully"