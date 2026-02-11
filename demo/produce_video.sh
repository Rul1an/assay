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

# Shot 1: Violation (Terminal — first 2.5s of hero.mp4)
echo "  Shot 1: Violation (2.5s)"
ffmpeg -y -i "$OUTPUT_DIR/hero.mp4" -ss 0 -t 2.5 -c:v libx264 -pix_fmt yuv420p "$TEMP_DIR/shot01.mp4" -loglevel error

# Shot 2: CI Blocks (VHS Simulation)
echo "  Shot 2: CI Blocks (Simulation)"
vhs demo/scenes/ci-failure.tape
# Copy generated file to temp
cp demo/scenes/ci-failure.mp4 "$TEMP_DIR/shot02.mp4"

# Shot 3: The Fix (VHS Simulation)
echo "  Shot 3: The Fix (Simulation)"
vhs demo/scenes/fix-policy.tape
cp demo/scenes/fix-policy.mp4 "$TEMP_DIR/shot03.mp4"

# Shot 4: Green Light (Terminal — PASS segment from hero.mp4)
echo "  Shot 4: Green Light (2.5s)"
ffmpeg -y -i "$OUTPUT_DIR/hero.mp4" -ss 2.8 -t 2.5 -c:v libx264 -pix_fmt yuv420p "$TEMP_DIR/shot04.mp4" -loglevel error

# Shot 5: Evidence Chain (VHS Simulation)
echo "  Shot 5: Evidence Chain (Simulation)"
vhs demo/scenes/merkle-chain.tape
cp demo/scenes/merkle-chain.mp4 "$TEMP_DIR/shot05.mp4"

# Shot 5b: Explore TUI (clip 2.5s of TUI navigation)
echo "  Shot 5b: Explore TUI (2.5s)"
vhs demo/explore.tape
ffmpeg -y -i demo/output/explore.mp4 -ss 1.5 -t 2.5 \
  -c:v libx264 -pix_fmt yuv420p "$TEMP_DIR/shot05b.mp4" -loglevel error

# Shot 6: Crypto Proof (Terminal)
echo "  Shot 6: Crypto Proof"
ffmpeg -y -i "$OUTPUT_DIR/evidence-lint.mp4" -ss 0.5 -t 6 -c:v libx264 -pix_fmt yuv420p "$TEMP_DIR/shot06.mp4" -loglevel error

# Shot 7: Attack Sim (Terminal)
echo "  Shot 7: Attack Sim"
ffmpeg -y -i "$OUTPUT_DIR/sim.mp4" -ss 0.5 -t 6 -c:v libx264 -pix_fmt yuv420p "$TEMP_DIR/shot07.mp4" -loglevel error

# Shot 8: Shield (VHS Simulation)
echo "  Shot 8: Shield (Simulation)"
vhs demo/scenes/shield.tape
cp demo/scenes/shield.mp4 "$TEMP_DIR/shot08.mp4"

# Shot 9: CTA Value Props (ffmpeg drawtext — clean typography)
echo "  Shot 9: Value Props (7s)"
ffmpeg -y -f lavfi -i "color=c=0x1E1E2E:s=1280x720:d=7:r=30" \
  -vf "drawtext=font=Menlo:text='One install.':fontcolor=white:fontsize=56:\
x=(w-text_w)/2:y=(h-text_h)/2:enable='between(t,0,1.5)',\
drawtext=font=Menlo:text='No signup.':fontcolor=white:fontsize=56:\
x=(w-text_w)/2:y=(h-text_h)/2:enable='between(t,1.8,3.3)',\
drawtext=font=Menlo:text='Runs offline.':fontcolor=white:fontsize=56:\
x=(w-text_w)/2:y=(h-text_h)/2:enable='between(t,3.6,5.1)',\
drawtext=font=Menlo:text='cargo install assay':fontcolor=0x89DCEB:fontsize=40:\
x=(w-text_w)/2:y=(h-text_h)/2:enable='between(t,5.4,7)'" \
  -c:v libx264 -crf 18 -pix_fmt yuv420p \
  "$TEMP_DIR/shot09.mp4" -loglevel error

# Shot 10: End Card (ffmpeg drawtext — centered, clean)
echo "  Shot 10: End Card (5s)"
ffmpeg -y -f lavfi -i "color=c=0x1E1E2E:s=1280x720:d=5:r=30" \
  -vf "drawtext=font=Menlo:text='ASSAY':fontcolor=white:fontsize=72:\
x=(w-text_w)/2:y=(h/2)-60,\
drawtext=font=Menlo:text='Security for the Agentic Age':fontcolor=0xBAC2DE:fontsize=28:\
x=(w-text_w)/2:y=(h/2)+10,\
drawtext=font=Menlo:text='github.com/Rul1an/assay':fontcolor=0x89DCEB:fontsize=24:\
x=(w-text_w)/2:y=(h/2)+60" \
  -c:v libx264 -crf 18 -pix_fmt yuv420p \
  "$TEMP_DIR/shot10.mp4" -loglevel error


#--- 2. NORMALIZE + ASSEMBLY ---

echo "🎞️ Normalizing shots (1280x720, 30fps, H.264)..."

NORM_DIR="$TEMP_DIR/normalized"
mkdir -p "$NORM_DIR"

SHOT_ORDER=(shot01 shot02 shot03 shot04 shot05 shot05b shot06 shot07 shot08 shot09 shot10)
for s in "${SHOT_ORDER[@]}"; do
    if [ -f "$TEMP_DIR/${s}.mp4" ]; then
        echo "  Normalizing: ${s}"
        ffmpeg -y -i "$TEMP_DIR/${s}.mp4" \
          -vf "scale=1280:720:force_original_aspect_ratio=decrease,pad=1280:720:(ow-iw)/2:(oh-ih)/2:color=#1E1E2E,fps=30,setsar=1" \
          -c:v libx264 -crf 18 -preset fast -pix_fmt yuv420p \
          -an "$NORM_DIR/${s}.mp4" -loglevel error
    fi
done

echo "✂️  Assembling timeline..."

cat > "$NORM_DIR/concat.txt" << EOF
file 'shot01.mp4'
file 'shot02.mp4'
file 'shot03.mp4'
file 'shot04.mp4'
file 'shot05.mp4'
file 'shot05b.mp4'
file 'shot06.mp4'
file 'shot07.mp4'
file 'shot08.mp4'
file 'shot09.mp4'
file 'shot10.mp4'
EOF

# Concat normalized shots (all same codec/res/fps now)
ffmpeg -y -f concat -safe 0 -i "$NORM_DIR/concat.txt" -c copy "$TEMP_DIR/raw.mp4" -loglevel error

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
Explore everything interactively. See exactly what happened. \
[[slnc 500]] \
Merkle root, SHA-256, JCS canonicalization. Cryptographic proof of what your agent actually did. \
[[slnc 500]] \
Attack simulation tests your gates against known vectors. Bitflip, truncation, injection, blocked or bypassed, you'll know. \
[[slnc 1000]] \
One install. No signup. Runs offline. \
[[slnc 1000]] \
cargo install assay. \
[[slnc 5000]]"

echo "  Using macOS 'say' (System TTS)..."
say -v Samantha -r 140 "$SCRIPT_TEXT" -o "$TEMP_DIR/voiceover.aiff"
ffmpeg -y -i "$TEMP_DIR/voiceover.aiff" "$TEMP_DIR/voiceover.wav" -loglevel error

#--- 4. FINAL MIX ---

echo "🎹 Final mix (color grade + fades + encoding)..."

TOTAL_DURATION=$(ffprobe -v error -show_entries format=duration \
  -of default=noprint_wrappers=1:nokey=1 "$TEMP_DIR/raw.mp4")
FADE_OUT_START=$(echo "$TOTAL_DURATION - 2" | bc)

ffmpeg -y \
  -i "$TEMP_DIR/raw.mp4" \
  -i "$TEMP_DIR/voiceover.wav" \
  -vf "eq=saturation=1.1:gamma_b=1.05:gamma_r=0.95,fade=t=in:st=0:d=1,fade=t=out:st=$FADE_OUT_START:d=2" \
  -af "afade=t=in:st=0:d=1,afade=t=out:st=$FADE_OUT_START:d=2" \
  -c:v libx264 -crf 18 -preset slow -pix_fmt yuv420p \
  -c:a aac -b:a 192k \
  -movflags +faststart \
  -shortest \
  "$FINAL_VIDEO" -loglevel error

# Summary
FINAL_DUR=$(ffprobe -v error -show_entries format=duration \
  -of default=noprint_wrappers=1:nokey=1 "$FINAL_VIDEO" | cut -c1-5)
FINAL_SIZE=$(du -h "$FINAL_VIDEO" | cut -f1)

echo ""
echo "═══════════════════════════════════════════════"
echo "✅ DONE: $FINAL_VIDEO"
echo "   Duration: ${FINAL_DUR}s"
echo "   Size:     ${FINAL_SIZE}"
echo "   Video:    1280x720 30fps H.264 CRF18"
echo "   Audio:    48kHz AAC 192k"
echo "   Polish:   color grade + fades + faststart"
echo "═══════════════════════════════════════════════"
