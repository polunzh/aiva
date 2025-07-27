#!/bin/bash
VM_NAME="{{ vm_name }}"
FORCE_FLAG="{{ force_flag }}"
VM_DIR="/var/lib/firecracker/$VM_NAME"

# Stop Firecracker process
if [ -f "$VM_DIR/firecracker.pid" ]; then
    FC_PID=$(cat "$VM_DIR/firecracker.pid")
    sudo kill $FORCE_FLAG $FC_PID 2>/dev/null || true
    sudo rm -f "$VM_DIR/firecracker.pid"
fi

# Clean up
sudo pkill -f "firecracker.*$VM_NAME" || true
sudo rm -f "$VM_DIR/firecracker.sock"
sudo ip link delete tap-$VM_NAME 2>/dev/null || true

echo "VM stopped successfully"