#!/bin/bash
set -e

# Requirement check
if ! command -v asciinema &> /dev/null; then
    echo "Error: asciinema is not installed."
    exit 1
fi

if ! python3 -m termtosvg --help &> /dev/null; then
    echo "Error: termtosvg is not installed (pip3 install --user termtosvg)."
    exit 1
fi

echo "Generating Hero SVG..."
# 1. Record cast via expect (ensures typing simulation)
chmod +x demo/record_hero.expect
./demo/record_hero.expect

# 2. Render to SVG
python3 -m termtosvg render demo/output/hero.cast demo/output/hero.svg

echo "Done: demo/output/hero.svg"
