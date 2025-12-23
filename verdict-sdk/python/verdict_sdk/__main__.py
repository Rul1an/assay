import sys

from .doctor import run_doctor


def main():
    if len(sys.argv) < 2:
        print("Usage: verdict-sdk <command>")
        print("Commands:")
        print("  doctor   Run health checks")
        sys.exit(1)

    cmd = sys.argv[1]
    if cmd == "doctor":
        run_doctor()
    elif cmd == "enforce":
        # Placeholder for future Phase 3.1
        print("Not implemented yet.")
    else:
        print(f"Unknown command: {cmd}")
        sys.exit(1)


if __name__ == "__main__":
    main()
