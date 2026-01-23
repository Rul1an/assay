#!/usr/bin/env python3
import pathlib
import sys
import re

def main():
    p = pathlib.Path("/etc/apt/sources.list.d/ubuntu.sources")
    if not p.exists():
        # Not a deb822 system (or file missing), nothing to do for this file
        sys.exit(0)

    print(f"Processing {p} for multiarch safety...")
    s = p.read_text()
    # Split by double newline to handle stanzas
    stanzas = re.split(r"\n\s*\n", s.strip())
    out = []

    for st in stanzas:
        if not st.strip():
            continue

        # Only touch deb stanzas (including "Types: deb deb-src")
        # We look for 'Types:' followed by 'deb' token
        if re.search(r"^Types:\s*.*\bdeb\b.*$", st, re.M):
            # Check if Architectures is already present
            if re.search(r"^Architectures:\s*", st, re.M):
                # Replace existing Architecture line with strict amd64
                st = re.sub(r"^Architectures:\s*.*$", "Architectures: amd64", st, flags=re.M)
            else:
                # Add Architectures: amd64
                st = st + "\nArchitectures: amd64"

        out.append(st)

    # Write back with double newline separation
    p.write_text("\n\n".join(out) + "\n")
    print(f"Successfully patched {p}")

if __name__ == "__main__":
    main()
