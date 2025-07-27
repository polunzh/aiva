#!/bin/bash
set -e

VM_NAME="{{ vm_name }}"
VM_DIR="/var/lib/firecracker/$VM_NAME"
SOCKET_PATH="$VM_DIR/firecracker.sock"
CONFIG_PATH="$VM_DIR/config.json"

# Create TAP device
sudo ip tuntap add tap-$VM_NAME mode tap 2>/dev/null || true
sudo ip addr add 172.16.0.1/24 dev tap-$VM_NAME 2>/dev/null || true
sudo ip link set dev tap-$VM_NAME up

# Kill any existing Firecracker process
sudo pkill -f "firecracker.*$VM_NAME" || true
sudo rm -f "$SOCKET_PATH"

# Start Firecracker
nohup sudo firecracker --api-sock "$SOCKET_PATH" > "$VM_DIR/firecracker.log" 2>&1 &
FC_PID=$!
echo $FC_PID | sudo tee "$VM_DIR/firecracker.pid" > /dev/null

# Wait for socket
for i in {1..30}; do
    if [ -S "$SOCKET_PATH" ]; then
        break
    fi
    sleep 0.2
done

# Configure and start VM using the config
CONFIG=$(cat "$CONFIG_PATH")
VCPU_COUNT=$(echo "$CONFIG" | jq -r ".vcpu_count")
MEM_SIZE=$(echo "$CONFIG" | jq -r ".mem_size_mib")
KERNEL_PATH=$(echo "$CONFIG" | jq -r ".kernel_path")
ROOTFS_PATH=$(echo "$CONFIG" | jq -r ".rootfs_path")
KERNEL_ARGS=$(echo "$CONFIG" | jq -r ".kernel_args")

# Configure machine
sudo curl -X PUT "http://localhost/machine-config" --unix-socket "$SOCKET_PATH" -H "Content-Type: application/json" -d "{\"vcpu_count\": $VCPU_COUNT, \"mem_size_mib\": $MEM_SIZE}"

# Configure boot source
sudo curl -X PUT "http://localhost/boot-source" --unix-socket "$SOCKET_PATH" -H "Content-Type: application/json" -d "{\"kernel_image_path\": \"$KERNEL_PATH\", \"boot_args\": \"$KERNEL_ARGS\"}"

# Configure drive
sudo curl -X PUT "http://localhost/drives/rootfs" --unix-socket "$SOCKET_PATH" -H "Content-Type: application/json" -d "{\"drive_id\": \"rootfs\", \"path_on_host\": \"$ROOTFS_PATH\", \"is_root_device\": true, \"is_read_only\": false}"

# Configure network
sudo curl -X PUT "http://localhost/network-interfaces/eth0" --unix-socket "$SOCKET_PATH" -H "Content-Type: application/json" -d "{\"iface_id\": \"eth0\", \"host_dev_name\": \"tap-$VM_NAME\"}"

# Start the instance
sudo curl -X PUT "http://localhost/actions" --unix-socket "$SOCKET_PATH" -H "Content-Type: application/json" -d "{\"action_type\": \"InstanceStart\"}"

echo "VM started successfully"