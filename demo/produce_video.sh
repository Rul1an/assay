#!/bin/bash
set -e

# Constants
OUTPUT_DIR="demo/output"
TEMP_DIR="demo/output/tmp_video"
FINAL_VIDEO="$OUTPUT_DIR/assay_hero_final.mp4"
mkdir -p "$TEMP_DIR"

echo "🎬 Starting Assay Hero Demo Production (Standard Definition)..."

#--- HELPER FUNCTIONS ---

generate_vhs_shot() {
    local text="$1"
    local duration="$2"
    local output="$3"
    local base_name
    base_name=$(basename "$output" .mp4)
    local tape="$TEMP_DIR/$base_name.tape"

    echo "  Generating placeholder (VHS): $text ($duration s)"
    cat > "$tape" <<EOF
Output $output
Set FontSize 40
Set Width 1280
Set Height 720
Set Padding 40
Set Theme "Catppuccin Mocha"
Hide
Type "AI B-ROLL PLACEHOLDER"
Enter
Show
Type "$text"
Sleep ${duration}s
EOF
    vhs "$tape"
}

#--- 1. SHOT GENERATION ---

echo "📸 Generating Shots..."

# Shot 1: Violation (Terminal)
echo "  Shot 1: Violation"
ffmpeg -y -i "$OUTPUT_DIR/hero.mp4" -ss 0 -t 3 -c:v libx264 -pix_fmt yuv420p "$TEMP_DIR/shot01.mp4" -loglevel error

# Shot 2: CI Blocks (VHS)
generate_vhs_shot "CI BLOCKED (Red Glitch)" 2.0 "$TEMP_DIR/shot02.mp4"

# Shot 3: The Fix (VHS)
generate_vhs_shot "DEVELOPER FIXING (Red -> Green)" 2.5 "$TEMP_DIR/shot03.mp4"

# Shot 4: Green Light (Terminal)
echo "  Shot 4: Green Light"
ffmpeg -y -i "$OUTPUT_DIR/hero.mp4" -ss 3.5 -t 2.5 -c:v libx264 -pix_fmt yuv420p "$TEMP_DIR/shot04.mp4" -loglevel error

# Shot 5: Evidence Chain (VHS)
generate_vhs_shot "MERKLE TREE (Adding Nodes)" 5.0 "$TEMP_DIR/shot05.mp4"

# Shot 6: Crypto Proof (Terminal)
echo "  Shot 6: Crypto Proof"
ffmpeg -y -i "$OUTPUT_DIR/evidence-lint.mp4" -ss 0.5 -t 6 -c:v libx264 -pix_fmt yuv420p "$TEMP_DIR/shot06.mp4" -loglevel error

# Shot 7: Attack Sim (Terminal)
echo "  Shot 7: Attack Sim"
ffmpeg -y -i "$OUTPUT_DIR/sim.mp4" -ss 0.5 -t 6 -c:v libx264 -pix_fmt yuv420p "$TEMP_DIR/shot07.mp4" -loglevel error

# Shot 8: Shield (VHS)
generate_vhs_shot "SHIELD BARRIER (Absorbing Attacks)" 3.0 "$TEMP_DIR/shot08.mp4"

# Shot 9: CTA Value Props (Special VHS)
echo "  Shot 9: Value Props (VHS)"
cat > "$TEMP_DIR/shot09.tape" <<EOF
Output $TEMP_DIR/shot09.mp4
Set FontSize 60
Set Width 1280
Set Height 720
Set Padding 40
Set Theme "Catppuccin Mocha"
Type "One install."
Sleep 1.5s
Enter
Type "No signup."
Sleep 1.5s
Enter
Type "Runs offline."
Sleep 1.5s
Enter
Type "cargo install assay"
Sleep 1.5s
EOF
vhs "$TEMP_DIR/shot09.tape"

# Shot 10: End Card (Special VHS)
echo "  Shot 10: End Card (VHS)"
cat > "$TEMP_DIR/shot10.tape" <<EOF
Output $TEMP_DIR/shot10.mp4
Set FontSize 80
Set Width 1280
Set Height 720
Set Padding 40
Set Theme "Catppuccin Mocha"
Type "ASSAY"
Enter
Set FontSize 40
Type "github.com/Rul1an/assay"
Sleep 4s
EOF
vhs "$TEMP_DIR/shot10.tape"


#--- 2. ASSEMBLY ---

echo "🎞️ Assembling Timeline..."

cat > "$TEMP_DIR/concat.txt" << EOF
file 'shot01.mp4'
file 'shot02.mp4'
file 'shot03.mp4'
file 'shot04.mp4'
file 'shot05.mp4'
file 'shot06.mp4'
file 'shot07.mp4'
file 'shot08.mp4'
file 'shot09.mp4'
file 'shot10.mp4'
EOF

# Concat to raw video
ffmpeg -y -f concat -safe 0 -i "$TEMP_DIR/concat.txt" -c copy "$TEMP_DIR/raw.mp4" -loglevel error

#--- 3. VOICEOVER ---

echo "🎙️ Generating Voiceover..."

SCRIPT_TEXT="Your AI agent just called a tool it shouldn't have. \
[[slnc 1000]] \
Assay catches it. Exit code one. CI blocks the deploy. \
[[slnc 500]] \
Fix the policy. Run it again. \
[[slnc 500]] \
Green. Deterministic. Same trace, same result, every time. \
[[slnc 1000]] \
Every action your agent takes becomes a signed event in a content-addressed evidence bundle. \
[[slnc 500]] \
Merkle root, SHA-256, JCS canonicalization. Cryptographic proof of what your agent actually did. \
[[slnc 500]] \
Attack simulation tests your gates against known vectors. Bitflip, truncation, injection, blocked or bypassed, you'll know. \
[[slnc 1000]] \
One install. No signup. Runs offline. \
[[slnc 1000]] \
cargo install assay."

echo "  Using macOS 'say' (System TTS)..."
say -v Samantha -r 180 "$SCRIPT_TEXT" -o "$TEMP_DIR/voiceover.aiff"
ffmpeg -y -i "$TEMP_DIR/voiceover.aiff" "$TEMP_DIR/voiceover.wav" -loglevel error

#--- 4. FINAL MIX ---

echo "🎹 Mixing Audio/Video..."

# Mix with shortest priority
ffmpeg -y -i "$TEMP_DIR/raw.mp4" -i "$TEMP_DIR/voiceover.wav" \
  -c:v copy -c:a aac -b:a 192k \
  -shortest "$FINAL_VIDEO" -loglevel error

echo "✅ Done! Output: $FINAL_VIDEO"
