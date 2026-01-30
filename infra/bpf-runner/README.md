# BPF-LSM Runner (Multipass / self-hosted)

Setup and maintenance for the GitHub Actions self-hosted runner used by Kernel Matrix CI (eBPF/LSM tests).

## Quick start

- **New VM (Mac):** `./setup_local_multipass.sh` then register the runner (see script output).
- **Cloud/VM:** `./setup.sh` (requires `GITHUB_TOKEN`, `RUNNER_NAME`, `REPO_URL`).

## "No space left on device" (Kernel Matrix CI)

If the **Kernel Matrix (5.15)** or **(6.6)** job fails with `Error: No space left on device`, the runner VM is out of disk. The error often happens during "Download action repository" (before any job step runs).

**Fix: free space on the VM**

1. **From the host (Multipass VM name `assay-bpf-runner`):**
   ```bash
   multipass exec assay-bpf-runner -- sudo bash -s < infra/bpf-runner/free_disk.sh
   ```
2. **If still full:** SSH into the VM (`multipass shell assay-bpf-runner`), stop the runner, clear work dirs, then start again:
   ```bash
   sudo su - github-runner -c "cd /opt/actions-runner && sudo ./svc.sh stop"
   sudo rm -rf /opt/actions-runner/_work/*
   sudo su - github-runner -c "cd /opt/actions-runner && sudo ./svc.sh start"
   ```
3. **New VMs:** In `setup_local_multipass.sh` you can increase `VM_DISK="20G"` to e.g. `40G` before creating the VM.

## Files

| File | Purpose |
|------|---------|
| `cloud-init.yaml` | VM provisioning (packages, Rust, BPF LSM kernel params). |
| `setup_local_multipass.sh` | Create/start Multipass VM (Mac). |
| `setup.sh` | Full runner setup on Ubuntu (cloud or existing VM). |
| `register_local.sh` | Register/update runner using a token. |
| `free_disk.sh` | Free disk on the runner VM (run when you see "No space left on device"). |
