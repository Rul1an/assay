# BPF-LSM Runner (Multipass / self-hosted)

Setup and maintenance for the GitHub Actions self-hosted runner used by Kernel Matrix CI (eBPF/LSM tests).

## Quick start

- **New VM (Mac):** `./setup_local_multipass.sh` then register the runner (see script output).
- **Cloud/VM:** `./setup.sh` (requires `GITHUB_TOKEN`, `RUNNER_NAME`, `REPO_URL`).

## "No space left on device" (Kernel Matrix CI)

If the **Kernel Matrix (5.15)** or **(6.6)** job fails with `Error: No space left on device`, the runner VM is out of disk. The error often appears at **"Prepare workflow directory"** or **"Download action repository"** — i.e. *before* any job step runs, so in-workflow cleanup cannot run. You must free space from the host or inside the VM.

**Fix: free space on the VM**

1. **From the host (Multipass VM name `assay-bpf-runner`):**
   ```bash
   multipass exec assay-bpf-runner -- sudo bash -s < infra/bpf-runner/free_disk.sh
   ```
   Then re-run the failed workflow. If the error was at "Prepare workflow directory", the runner needs space to create the job dir and download actions; after freeing, the next run can proceed.

2. **If still full:** Wipe the runner work directory (any in-flight job will fail; next run can start clean):
   ```bash
   multipass exec assay-bpf-runner -- sudo rm -rf /opt/actions-runner/_work/*
   ```
   Optionally stop/start the runner service (if you have SSH or exec access):
   ```bash
   multipass exec assay-bpf-runner -- sudo su - github-runner -c "cd /opt/actions-runner && sudo ./svc.sh stop"
   multipass exec assay-bpf-runner -- sudo rm -rf /opt/actions-runner/_work/*
   multipass exec assay-bpf-runner -- sudo su - github-runner -c "cd /opt/actions-runner && sudo ./svc.sh start"
   ```
3. **New VMs:** In `setup_local_multipass.sh` the default is `VM_DISK="30G"`. Increase to e.g. `40G` if you still hit "No space left" after cleanup.

**Automatic cleanup (always before matrix + hourly on VM):**

- **Workflow:** A job `Free disk (before matrix)` runs on the self-hosted runner before each Kernel Matrix job and frees Docker/APT/runner caches. So every run starts with a cleanup.
- **Hourly cron on VM:** New setups (`./setup.sh` or `./setup_local_multipass.sh`) install a root cron that runs `free_disk.sh` every hour. For an **existing** Multipass VM, install the cron once from the host:
  ```bash
  multipass exec assay-bpf-runner -- sudo mkdir -p /opt/actions-runner/scripts
  multipass exec assay-bpf-runner -- sudo tee /opt/actions-runner/scripts/free_disk.sh < infra/bpf-runner/free_disk.sh
  multipass exec assay-bpf-runner -- sudo chmod +x /opt/actions-runner/scripts/free_disk.sh
  multipass exec assay-bpf-runner -- sudo bash -c '(crontab -l 2>/dev/null | grep -q free_disk) || (crontab -l 2>/dev/null; echo "0 * * * * /opt/actions-runner/scripts/free_disk.sh >> /var/log/assay-free_disk.log 2>&1") | crontab -'
  ```

## Files

| File | Purpose |
|------|---------|
| `cloud-init.yaml` | VM provisioning (packages, Rust, BPF LSM kernel params). |
| `setup_local_multipass.sh` | Create/start Multipass VM (Mac). |
| `setup.sh` | Full runner setup on Ubuntu (cloud or existing VM). |
| `register_local.sh` | Register/update runner using a token. |
| `free_disk.sh` | Free disk on the runner VM (run when you see "No space left on device"). |
