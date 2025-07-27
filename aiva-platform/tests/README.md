# AIVA Platform Tests

This directory contains unit and integration tests for the AIVA platform abstraction layer.

## Test Structure

### Unit Tests (src/tests/)
- `command_pool_tests.rs` - Tests for VM command execution pool
- `vsock_executor_tests.rs` - Tests for vsock/network command execution
- `platform_tests.rs` - Platform-specific unit tests

### Integration Tests (tests/)
- `integration_tests.rs` - Full VM lifecycle and cross-platform tests

## Running Tests

### Run all tests
```bash
cargo test
```

### Run only unit tests
```bash
cargo test --lib
```

### Run integration tests (including ignored tests)
```bash
# Run non-ignored integration tests
cargo test --test integration_tests

# Run ALL tests including those requiring setup
cargo test --test integration_tests -- --ignored
```

### Platform-specific tests
```bash
# Linux-specific tests
cargo test --features linux

# macOS-specific tests  
cargo test --features macos

# Windows-specific tests
cargo test --features windows
```

## Test Requirements

### Linux
- KVM support (`/dev/kvm` accessible)
- Firecracker binary installed
- User in `kvm` group or `/dev/kvm` permissions

### macOS
- Lima installed (`brew install lima`)
- Sufficient disk space for Lima VMs

### Windows
- Windows 10/11 with WSL 2 enabled
- Ubuntu or similar Linux distribution in WSL
- Nested virtualization support

## Integration Test Setup

Many integration tests are marked with `#[ignore]` because they require:

1. **Platform Setup**: Lima on macOS, WSL on Windows, KVM on Linux
2. **Firecracker**: The Firecracker binary and kernel/rootfs images
3. **Permissions**: Appropriate permissions for virtualization
4. **Network**: TAP device creation permissions

To run these tests:

1. Ensure your platform is properly configured
2. Download Firecracker and VM images
3. Run with `--ignored` flag

## Mocking and Test Helpers

The tests include helpers for:
- Creating test VM configurations
- Mocking network connections
- Platform detection
- Error scenario testing

## Troubleshooting

### Linux
- Check KVM: `ls -la /dev/kvm`
- Check permissions: `groups` (should include `kvm`)
- Check Firecracker: `which firecracker`

### macOS
- Check Lima: `limactl list`
- Check disk space: `df -h`
- Check Lima VM: `limactl shell aiva-host`

### Windows
- Check WSL: `wsl --status`
- Check distro: `wsl --list`
- Check nested virt: Run in PowerShell as admin:
  ```powershell
  Get-ComputerInfo -Property HyperVisorPresent
  ```