#!/bin/bash
# Generate AI narration for hero video
# Usage: ./demo/narrate.sh

# Script: Concise, punchy, timed to ~9s
TEXT="This is Assay. Run your agent. Oh, it failed? Fix the policy. Run it again. Safe. Deterministic. Secure."

# Generate audio (using Samantha if available, fallback to default)
VOICE=""
if say -v Samantha "test" >/dev/null 2>&1; then
    VOICE="-v Samantha"
fi

echo "Generating narration audio..."
say $VOICE -r 170 "$TEXT" -o demo/output/narration.aiff

# Merge with video
# -y: overwrite
# -shortest: stop when shortest stream ends (usually video)
echo "Merging with video..."
ffmpeg -y -v error -i demo/output/hero.mp4 -i demo/output/narration.aiff -c:v copy -c:a aac -shortest -map 0:v:0 -map 1:a:0 demo/output/hero-narrated.mp4

echo "Done: demo/output/hero-narrated.mp4"
