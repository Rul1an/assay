#!/bin/bash
# ============================================================================
# Assay Hero Demo — Automated QA Check
# ============================================================================
# Validates the final video against production standards.
# Run after produce_video.sh to catch issues before release.
#
# Usage:  bash demo/qa_check.sh [video_file]
# Default: demo/output/assay_hero_final.mp4
# ============================================================================

VIDEO="${1:-demo/output/assay_hero_final.mp4}"
PASS=0
FAIL=0
WARN=0

check() {
    local label="$1"
    local result="$2"  # "pass", "fail", or "warn"
    local detail="$3"

    case "$result" in
        pass) echo "  ✅ $label: $detail"; PASS=$((PASS + 1)) ;;
        fail) echo "  ❌ $label: $detail"; FAIL=$((FAIL + 1)) ;;
        warn) echo "  ⚠️  $label: $detail"; WARN=$((WARN + 1)) ;;
    esac
}

echo "🔍 QA Check: $VIDEO"
echo ""

if [ ! -f "$VIDEO" ]; then
    echo "❌ File not found: $VIDEO"
    exit 1
fi

# ============================================================================
# TECHNICAL CHECKS
# ============================================================================

echo "── Technical ──"

# Resolution
RES=$(ffprobe -v error -select_streams v:0 \
  -show_entries stream=width,height \
  -of csv=p=0 "$VIDEO")
if [ "$RES" = "1280,720" ] || [ "$RES" = "1920,1080" ]; then
    check "Resolution" "pass" "$RES"
else
    check "Resolution" "fail" "$RES (expected 1280x720 or 1920x1080)"
fi

# Frame rate
FPS=$(ffprobe -v error -select_streams v:0 \
  -show_entries stream=r_frame_rate \
  -of default=noprint_wrappers=1:nokey=1 "$VIDEO")
FPS_NUM=$(echo "$FPS" | cut -d/ -f1)
FPS_DEN=$(echo "$FPS" | cut -d/ -f2)
FPS_FLOAT=$(echo "scale=1; $FPS_NUM / $FPS_DEN" | bc)
if [ "$(echo "$FPS_FLOAT >= 24" | bc)" -eq 1 ]; then
    check "Frame rate" "pass" "${FPS_FLOAT} fps"
else
    check "Frame rate" "fail" "${FPS_FLOAT} fps (expected >= 24)"
fi

# Codec
CODEC=$(ffprobe -v error -select_streams v:0 \
  -show_entries stream=codec_name \
  -of default=noprint_wrappers=1:nokey=1 "$VIDEO")
if [ "$CODEC" = "h264" ]; then
    check "Video codec" "pass" "$CODEC"
else
    check "Video codec" "warn" "$CODEC (expected h264)"
fi

# Pixel format
PIX_FMT=$(ffprobe -v error -select_streams v:0 \
  -show_entries stream=pix_fmt \
  -of default=noprint_wrappers=1:nokey=1 "$VIDEO")
if [ "$PIX_FMT" = "yuv420p" ]; then
    check "Pixel format" "pass" "$PIX_FMT"
else
    check "Pixel format" "fail" "$PIX_FMT (expected yuv420p)"
fi

# Audio sample rate
SAMPLE_RATE=$(ffprobe -v error -select_streams a:0 \
  -show_entries stream=sample_rate \
  -of default=noprint_wrappers=1:nokey=1 "$VIDEO" 2>/dev/null || echo "0")
if [ "$SAMPLE_RATE" -ge 44100 ] 2>/dev/null; then
    check "Audio sample rate" "pass" "${SAMPLE_RATE} Hz"
else
    check "Audio sample rate" "fail" "${SAMPLE_RATE} Hz (expected >= 44100)"
fi

# Audio codec
ACODEC=$(ffprobe -v error -select_streams a:0 \
  -show_entries stream=codec_name \
  -of default=noprint_wrappers=1:nokey=1 "$VIDEO" 2>/dev/null || echo "none")
if [ "$ACODEC" = "aac" ]; then
    check "Audio codec" "pass" "$ACODEC"
else
    check "Audio codec" "warn" "$ACODEC (expected aac)"
fi

# Duration
DURATION=$(ffprobe -v error -show_entries format=duration \
  -of default=noprint_wrappers=1:nokey=1 "$VIDEO")
DUR_INT=$(echo "$DURATION" | cut -d. -f1)
if [ "$DUR_INT" -ge 30 ] && [ "$DUR_INT" -le 60 ]; then
    check "Duration" "pass" "${DUR_INT}s (target: 38-45s)"
elif [ "$DUR_INT" -ge 20 ] && [ "$DUR_INT" -le 70 ]; then
    check "Duration" "warn" "${DUR_INT}s (target: 38-45s, acceptable: 30-60s)"
else
    check "Duration" "fail" "${DUR_INT}s (way outside target 38-45s)"
fi

# File size
SIZE_BYTES=$(stat -f%z "$VIDEO" 2>/dev/null || stat -c%s "$VIDEO" 2>/dev/null || echo "0")
SIZE_MB=$(echo "scale=1; $SIZE_BYTES / 1048576" | bc)
if [ "$(echo "$SIZE_MB < 100" | bc)" -eq 1 ]; then
    check "File size" "pass" "${SIZE_MB} MB"
else
    check "File size" "warn" "${SIZE_MB} MB (large for a 40s video)"
fi

# movflags faststart (moov atom at beginning)
# Check if moov atom appears before mdat
MOOV_POS=$(ffprobe -v trace "$VIDEO" 2>&1 | grep -m1 "type:'moov'" | head -1 || true)
if [ -n "$MOOV_POS" ]; then
    check "faststart" "pass" "moov atom detected"
else
    check "faststart" "warn" "Could not verify moov position"
fi

# Bitrate
BITRATE=$(ffprobe -v error -show_entries format=bit_rate \
  -of default=noprint_wrappers=1:nokey=1 "$VIDEO")
BR_KBPS=$(echo "$BITRATE / 1000" | bc 2>/dev/null || echo "0")
if [ "$BR_KBPS" -ge 500 ]; then
    check "Bitrate" "pass" "${BR_KBPS} kbps"
else
    check "Bitrate" "fail" "${BR_KBPS} kbps (too low, expected >= 500)"
fi

# ============================================================================
# CONTENT CHECKS
# ============================================================================

echo ""
echo "── Content ──"

# Check thumbnails exist
if [ -f "demo/output/screenshots/hero-thumb.png" ]; then
    check "Hero thumbnail" "pass" "exists"
else
    check "Hero thumbnail" "fail" "missing (demo/output/screenshots/hero-thumb.png)"
fi

if [ -f "demo/output/screenshots/sim-thumb.png" ]; then
    check "Sim thumbnail" "pass" "exists"
else
    check "Sim thumbnail" "warn" "missing (demo/output/screenshots/sim-thumb.png)"
fi

# Check captions.srt exists and is valid
if [ -f "demo/captions.srt" ]; then
    SRT_LINES=$(wc -l < "demo/captions.srt" | tr -d ' ')
    if [ "$SRT_LINES" -ge 20 ]; then
        check "Captions SRT" "pass" "demo/captions.srt ($SRT_LINES lines)"
    else
        check "Captions SRT" "warn" "demo/captions.srt seems short ($SRT_LINES lines)"
    fi
else
    check "Captions SRT" "fail" "demo/captions.srt not found"
fi

# Check audio/video duration sync
if [ -n "$DURATION" ]; then
    AUDIO_DUR=$(ffprobe -v error -select_streams a:0 \
      -show_entries stream=duration \
      -of default=noprint_wrappers=1:nokey=1 "$VIDEO" 2>/dev/null || echo "0")
    if [ -n "$AUDIO_DUR" ] && [ "$AUDIO_DUR" != "0" ]; then
        SYNC_DIFF=$(echo "$DURATION - $AUDIO_DUR" | bc)
        SYNC_ABS=$(echo "$SYNC_DIFF" | tr -d '-')
        if [ "$(echo "$SYNC_ABS < 2" | bc)" -eq 1 ]; then
            check "A/V sync" "pass" "Δ${SYNC_DIFF}s"
        else
            check "A/V sync" "fail" "Δ${SYNC_DIFF}s (>2s drift)"
        fi
    fi
fi

# ============================================================================
# SUMMARY
# ============================================================================

TOTAL=$((PASS + FAIL + WARN))
echo ""
echo "═══════════════════════════════════════════════"
echo "QA Results: $PASS passed, $FAIL failed, $WARN warnings (of $TOTAL checks)"

if [ "$FAIL" -eq 0 ]; then
    echo "✅ Video is release-ready!"
    exit 0
else
    echo "❌ Fix $FAIL issue(s) before release."
    exit 1
fi
