import subprocess
import os

# Constants
ASSAY_BIN = "./target/release/assay"
SERVER_SCRIPT = "tests/echo_server.py"
POLICIES_DIR = "tests/security_audit/policies"
RESULTS_FILE = "tests/security_audit/results.log"

def run_test(name, input_json, policy_file=None):
    print(f"Running Test: {name}...", end=" ", flush=True)

    cmd = [ASSAY_BIN, "mcp", "wrap"]
    if policy_file:
        cmd.extend(["--policy", os.path.join(POLICIES_DIR, policy_file)])

    cmd.append("--verbose")
    cmd.append("--")
    # Use unbuffered python for the server to avoid deadlock waiting for output
    cmd.extend(["python3", "-u", SERVER_SCRIPT])

    env = os.environ.copy()
    env["RUST_BACKTRACE"] = "1"

    try:
        # Use subprocess.run with input/timeout which is safer than Popen manually
        result = subprocess.run(
            cmd,
            input=input_json + "\n",
            capture_output=True,
            text=True,
            timeout=5, # Strict timeout
            env=env
        )

        # Log results
        with open(RESULTS_FILE, "a") as f:
            f.write(f"\n=== Test: {name} ===\n")
            f.write(f"Input: {input_json}\n")
            f.write("--- STDOUT ---\n")
            f.write(result.stdout)
            f.write("\n--- STDERR ---\n")
            f.write(result.stderr)
            f.write(f"\nExit Code: {result.returncode}\n")

        print("DONE")
        return result.stdout, result.stderr

    except subprocess.TimeoutExpired as e:
        print("TIMEOUT")
        with open(RESULTS_FILE, "a") as f:
            f.write(f"\n=== Test: {name} ===\nTIMEOUT\n")
            # If we captured anything partial:
            if e.stdout: f.write(f"Partial STDOUT: {e.stdout.decode()}\n")
            if e.stderr: f.write(f"Partial STDERR: {e.stderr.decode()}\n")
        return None, None

    except Exception as e:
        print(f"ERROR: {e}")
        return None, None

def main():
    # Setup
    os.makedirs(os.path.dirname(RESULTS_FILE), exist_ok=True)
    with open(RESULTS_FILE, "w") as f:
        f.write("Security Audit Log (Retry)\n")

    # 1. Smoke
    run_test("Smoke", '{"jsonrpc": "2.0", "id": 1, "method": "ping"}')

    # 2. Deny
    run_test("Deny",
             '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"delete_file","arguments":{"path":"/etc/passwd"}}}',
             "denylist.yaml")

    # 3. Allow
    run_test("Allow",
             '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"write_file","arguments":{}}}',
             "allowlist.yaml")

    # 4. Constraints
    run_test("Constraints",
             '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"run_command","arguments":{"command":"rm -rf /"}}}',
             "constraints.yaml")

    print(f"\nResults saved to {RESULTS_FILE}")

if __name__ == "__main__":
    main()
