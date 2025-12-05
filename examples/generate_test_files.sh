#!/bin/bash
#
# Generate Test MP3 Files for Losselot Demo
#
# This script creates various MP3 files with different encoding scenarios
# to demonstrate Losselot's detection capabilities:
#
#   1. Clean files (legitimate encodes from lossless)
#   2. Transcoded files (128kbps source re-encoded as 320kbps)
#   3. Re-encoded files (multiple LAME passes)
#   4. Mixed encoder chain (LAME → FFmpeg)
#   5. YouTube-style rips (low quality source upscaled)
#
# Requirements: ffmpeg, lame, sox
#
# Usage: ./generate_test_files.sh
#

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUTPUT_DIR="$SCRIPT_DIR/demo_files"

log() { echo -e "${GREEN}[+]${NC} $1"; }
warn() { echo -e "${YELLOW}[!]${NC} $1"; }
error() { echo -e "${RED}[!]${NC} $1"; }

check_deps() {
    local missing=()
    command -v ffmpeg >/dev/null 2>&1 || missing+=("ffmpeg")
    command -v lame >/dev/null 2>&1 || missing+=("lame")

    if [[ ${#missing[@]} -gt 0 ]]; then
        error "Missing dependencies: ${missing[*]}"
        echo "Install with: brew install ${missing[*]}"
        exit 1
    fi
}

# Generate a test tone WAV file (source material)
generate_source_wav() {
    local output="$1"
    local duration="${2:-10}"

    log "Generating source WAV with full frequency content..."

    # Use ffmpeg to generate a complex test signal with:
    # - Multiple sine waves at different frequencies
    # - White noise for broadband content
    # - This simulates music-like frequency content
    ffmpeg -y \
        -f lavfi -i "sine=frequency=440:duration=$duration" \
        -f lavfi -i "sine=frequency=880:duration=$duration" \
        -f lavfi -i "sine=frequency=1760:duration=$duration" \
        -f lavfi -i "sine=frequency=3520:duration=$duration" \
        -f lavfi -i "sine=frequency=7040:duration=$duration" \
        -f lavfi -i "sine=frequency=14080:duration=$duration" \
        -f lavfi -i "anoisesrc=d=$duration:c=pink:a=0.1" \
        -filter_complex "[0][1][2][3][4][5][6]amix=inputs=7:duration=first[out]" \
        -map "[out]" \
        -ar 44100 \
        -sample_fmt s16 \
        "$output" 2>/dev/null

    log "Source WAV created: $output"
}

# 1. Clean legitimate 320kbps encode
create_clean_320() {
    local source="$1"
    local output="$OUTPUT_DIR/01_clean_320kbps.mp3"

    log "Creating clean 320kbps encode..."
    lame --preset insane -q 0 "$source" "$output" 2>/dev/null
    log "Created: $output"
}

# 2. Clean legitimate 192kbps VBR V0
create_clean_v0() {
    local source="$1"
    local output="$OUTPUT_DIR/02_clean_v0_vbr.mp3"

    log "Creating clean V0 VBR encode..."
    lame -V 0 -q 0 "$source" "$output" 2>/dev/null
    log "Created: $output"
}

# 3. Transcode: 128kbps source re-encoded as 320kbps (CLASSIC FAKE)
create_transcode_128_to_320() {
    local source="$1"
    local temp_128="$OUTPUT_DIR/.temp_128.mp3"
    local temp_wav="$OUTPUT_DIR/.temp_decoded.wav"
    local output="$OUTPUT_DIR/03_FAKE_128_to_320.mp3"

    log "Creating TRANSCODE: 128kbps → WAV → 320kbps..."

    # Step 1: Encode to 128kbps (creates 16kHz lowpass)
    lame -b 128 -q 5 "$source" "$temp_128" 2>/dev/null

    # Step 2: Decode back to WAV
    ffmpeg -y -i "$temp_128" "$temp_wav" 2>/dev/null

    # Step 3: Re-encode as "320kbps" - but lowpass is still 16kHz!
    lame --preset insane -q 0 "$temp_wav" "$output" 2>/dev/null

    rm -f "$temp_128" "$temp_wav"
    log "Created: $output (TRANSCODE - lowpass reveals 128kbps source)"
}

# 4. Re-encoded multiple times with LAME
create_multiple_lame_passes() {
    local source="$1"
    local temp1="$OUTPUT_DIR/.temp_pass1.mp3"
    local temp2="$OUTPUT_DIR/.temp_pass2.wav"
    local temp3="$OUTPUT_DIR/.temp_pass2.mp3"
    local temp4="$OUTPUT_DIR/.temp_pass3.wav"
    local output="$OUTPUT_DIR/04_REENCODED_3x_lame.mp3"

    log "Creating RE-ENCODED file: 3 LAME passes..."

    # Pass 1: Initial encode at 256kbps
    lame -b 256 -q 2 "$source" "$temp1" 2>/dev/null

    # Pass 2: Decode and re-encode at 320kbps
    ffmpeg -y -i "$temp1" "$temp2" 2>/dev/null
    lame --preset insane "$temp2" "$temp3" 2>/dev/null

    # Pass 3: Decode and re-encode again at 320kbps
    ffmpeg -y -i "$temp3" "$temp4" 2>/dev/null
    lame --preset insane "$temp4" "$output" 2>/dev/null

    rm -f "$temp1" "$temp2" "$temp3" "$temp4"
    log "Created: $output (3 LAME signatures detectable)"
}

# 5. Mixed encoder chain: LAME → FFmpeg
create_lame_to_ffmpeg_chain() {
    local source="$1"
    local temp_lame="$OUTPUT_DIR/.temp_lame.mp3"
    local temp_wav="$OUTPUT_DIR/.temp_ffmpeg.wav"
    local output="$OUTPUT_DIR/05_CHAIN_lame_to_ffmpeg.mp3"

    log "Creating ENCODING CHAIN: LAME → FFmpeg..."

    # Step 1: Encode with LAME
    lame -V 2 "$source" "$temp_lame" 2>/dev/null

    # Step 2: Decode
    ffmpeg -y -i "$temp_lame" "$temp_wav" 2>/dev/null

    # Step 3: Re-encode with FFmpeg (leaves Lavf signature)
    ffmpeg -y -i "$temp_wav" -codec:a libmp3lame -b:a 320k "$output" 2>/dev/null

    rm -f "$temp_lame" "$temp_wav"
    log "Created: $output (LAME + FFmpeg signatures)"
}

# 6. YouTube-style rip (128kbps AAC equivalent quality)
create_youtube_style() {
    local source="$1"
    local temp_aac="$OUTPUT_DIR/.temp_aac.m4a"
    local temp_wav="$OUTPUT_DIR/.temp_youtube.wav"
    local output="$OUTPUT_DIR/06_YOUTUBE_style_rip.mp3"

    log "Creating YOUTUBE-STYLE rip (128kbps AAC → 320kbps MP3)..."

    # Step 1: Encode as low-quality AAC (simulates YouTube audio)
    ffmpeg -y -i "$source" -c:a aac -b:a 128k "$temp_aac" 2>/dev/null

    # Step 2: Decode
    ffmpeg -y -i "$temp_aac" "$temp_wav" 2>/dev/null

    # Step 3: Re-encode as "high quality" MP3
    lame --preset insane "$temp_wav" "$output" 2>/dev/null

    rm -f "$temp_aac" "$temp_wav"
    log "Created: $output (low quality source upscaled)"
}

# 7. Multiple FFmpeg passes
create_multiple_ffmpeg_passes() {
    local source="$1"
    local temp1="$OUTPUT_DIR/.temp_ff1.mp3"
    local temp2="$OUTPUT_DIR/.temp_ff2.mp3"
    local output="$OUTPUT_DIR/07_FFMPEG_processed_3x.mp3"

    log "Creating file with multiple FFmpeg processings..."

    # Pass 1
    ffmpeg -y -i "$source" -codec:a libmp3lame -b:a 256k "$temp1" 2>/dev/null

    # Pass 2 (re-encode)
    ffmpeg -y -i "$temp1" -codec:a libmp3lame -b:a 320k "$temp2" 2>/dev/null

    # Pass 3 (re-encode again)
    ffmpeg -y -i "$temp2" -codec:a libmp3lame -b:a 320k "$output" 2>/dev/null

    rm -f "$temp1" "$temp2"
    log "Created: $output (multiple FFmpeg Lavf signatures)"
}

# 8. Borderline suspect (192kbps source to 256kbps)
create_borderline_suspect() {
    local source="$1"
    local temp_192="$OUTPUT_DIR/.temp_192.mp3"
    local temp_wav="$OUTPUT_DIR/.temp_border.wav"
    local output="$OUTPUT_DIR/08_SUSPECT_192_to_256.mp3"

    log "Creating borderline SUSPECT file (192 → 256kbps)..."

    # Encode at 192kbps
    lame -b 192 "$source" "$temp_192" 2>/dev/null

    # Decode and re-encode at 256kbps
    ffmpeg -y -i "$temp_192" "$temp_wav" 2>/dev/null
    lame -b 256 "$temp_wav" "$output" 2>/dev/null

    rm -f "$temp_192" "$temp_wav"
    log "Created: $output (borderline quality loss)"
}

# 9. Multi-codec "laundering" chain: WAV → MP3 128k → AAC → OGG → MP3 320k
# Simulates someone trying to hide source quality by converting through formats
create_multi_codec_laundering() {
    local source="$1"
    local temp_mp3_128="$OUTPUT_DIR/.temp_launder_mp3.mp3"
    local temp_wav1="$OUTPUT_DIR/.temp_launder_wav1.wav"
    local temp_aac="$OUTPUT_DIR/.temp_launder_aac.m4a"
    local temp_wav2="$OUTPUT_DIR/.temp_launder_wav2.wav"
    local temp_ogg="$OUTPUT_DIR/.temp_launder_ogg.ogg"
    local temp_wav3="$OUTPUT_DIR/.temp_launder_wav3.wav"
    local output="$OUTPUT_DIR/09_LAUNDERED_mp3_aac_ogg_mp3.mp3"

    log "Creating LAUNDERED file: WAV → MP3 128k → AAC → OGG → MP3 320k..."

    # Step 1: Encode to low-quality MP3 (128kbps) - this is the "source"
    lame -b 128 -q 5 "$source" "$temp_mp3_128" 2>/dev/null

    # Step 2: Decode and convert to AAC (someone "upgrading" to AAC)
    ffmpeg -y -i "$temp_mp3_128" "$temp_wav1" 2>/dev/null
    ffmpeg -y -i "$temp_wav1" -c:a aac -b:a 256k "$temp_aac" 2>/dev/null

    # Step 3: Decode AAC and convert to OGG Vorbis (more "upgrading")
    ffmpeg -y -i "$temp_aac" "$temp_wav2" 2>/dev/null
    ffmpeg -y -i "$temp_wav2" -c:a libvorbis -q:a 6 "$temp_ogg" 2>/dev/null

    # Step 4: Decode OGG and "upgrade" to 320kbps MP3 (final "high quality" output)
    ffmpeg -y -i "$temp_ogg" "$temp_wav3" 2>/dev/null
    lame --preset insane "$temp_wav3" "$output" 2>/dev/null

    rm -f "$temp_mp3_128" "$temp_wav1" "$temp_aac" "$temp_wav2" "$temp_ogg" "$temp_wav3"
    log "Created: $output (4 lossy encodings through 3 codecs - quality destroyed)"
}

# ══════════════════════════════════════════════════════════════════════════════
# ADDITIONAL TEST FILES FOR VARIETY
# ══════════════════════════════════════════════════════════════════════════════

# 10. Clean FLAC (true lossless)
create_clean_flac() {
    local source="$1"
    local folder="$2"
    local output="$folder/10_clean_lossless.flac"

    log "Creating clean FLAC (true lossless)..."
    ffmpeg -y -i "$source" -c:a flac "$output" 2>/dev/null
    log "Created: $output"
}

# 11. Transcoded FLAC (MP3 128k source disguised as FLAC)
create_transcode_flac() {
    local source="$1"
    local folder="$2"
    local temp_128="$OUTPUT_DIR/.temp_flac_128.mp3"
    local temp_wav="$OUTPUT_DIR/.temp_flac_decoded.wav"
    local output="$folder/11_FAKE_flac_from_128k.flac"

    log "Creating FAKE FLAC from 128kbps source..."
    lame -b 128 -q 5 "$source" "$temp_128" 2>/dev/null
    ffmpeg -y -i "$temp_128" "$temp_wav" 2>/dev/null
    ffmpeg -y -i "$temp_wav" -c:a flac "$output" 2>/dev/null

    rm -f "$temp_128" "$temp_wav"
    log "Created: $output (FLAC from 128kbps - fake lossless)"
}

# 12. Clean 256kbps
create_clean_256() {
    local source="$1"
    local folder="$2"
    local output="$folder/12_clean_256kbps.mp3"

    log "Creating clean 256kbps encode..."
    lame -b 256 -q 0 "$source" "$output" 2>/dev/null
    log "Created: $output"
}

# 13. Clean V2 VBR (~190kbps)
create_clean_v2() {
    local source="$1"
    local folder="$2"
    local output="$folder/13_clean_v2_vbr.mp3"

    log "Creating clean V2 VBR encode..."
    lame -V 2 -q 0 "$source" "$output" 2>/dev/null
    log "Created: $output"
}

# 14. 160kbps upscaled to 320kbps
create_transcode_160_to_320() {
    local source="$1"
    local folder="$2"
    local temp_160="$OUTPUT_DIR/.temp_160.mp3"
    local temp_wav="$OUTPUT_DIR/.temp_160_decoded.wav"
    local output="$folder/14_FAKE_160_to_320.mp3"

    log "Creating TRANSCODE: 160kbps → 320kbps..."
    lame -b 160 -q 5 "$source" "$temp_160" 2>/dev/null
    ffmpeg -y -i "$temp_160" "$temp_wav" 2>/dev/null
    lame --preset insane "$temp_wav" "$output" 2>/dev/null

    rm -f "$temp_160" "$temp_wav"
    log "Created: $output"
}

# 15. WAV (true lossless)
create_clean_wav() {
    local source="$1"
    local folder="$2"
    local output="$folder/15_clean_lossless.wav"

    log "Creating clean WAV (true lossless)..."
    cp "$source" "$output"
    log "Created: $output"
}

# 16. Fake WAV from low bitrate
create_transcode_wav() {
    local source="$1"
    local folder="$2"
    local temp_128="$OUTPUT_DIR/.temp_wav_128.mp3"
    local output="$folder/16_FAKE_wav_from_128k.wav"

    log "Creating FAKE WAV from 128kbps source..."
    lame -b 128 -q 5 "$source" "$temp_128" 2>/dev/null
    ffmpeg -y -i "$temp_128" "$output" 2>/dev/null

    rm -f "$temp_128"
    log "Created: $output (WAV from 128kbps - fake lossless)"
}

# 17. Clean 192kbps
create_clean_192() {
    local source="$1"
    local folder="$2"
    local output="$folder/17_clean_192kbps.mp3"

    log "Creating clean 192kbps encode..."
    lame -b 192 -q 0 "$source" "$output" 2>/dev/null
    log "Created: $output"
}

# 18. Clean 128kbps (legitimate low bitrate)
create_clean_128() {
    local source="$1"
    local folder="$2"
    local output="$folder/18_clean_128kbps.mp3"

    log "Creating clean 128kbps encode..."
    lame -b 128 -q 0 "$source" "$output" 2>/dev/null
    log "Created: $output"
}

# 19. FLAC from 320kbps (subtle damage)
create_flac_from_320() {
    local source="$1"
    local folder="$2"
    local temp_320="$OUTPUT_DIR/.temp_320.mp3"
    local temp_wav="$OUTPUT_DIR/.temp_320_decoded.wav"
    local output="$folder/19_SUSPECT_flac_from_320k.flac"

    log "Creating SUSPECT FLAC from 320kbps source..."
    lame --preset insane "$source" "$temp_320" 2>/dev/null
    ffmpeg -y -i "$temp_320" "$temp_wav" 2>/dev/null
    ffmpeg -y -i "$temp_wav" -c:a flac "$output" 2>/dev/null

    rm -f "$temp_320" "$temp_wav"
    log "Created: $output (FLAC from 320kbps - subtle damage)"
}

# 20. OGG Vorbis clean
create_clean_ogg() {
    local source="$1"
    local folder="$2"
    local output="$folder/20_clean_ogg_q6.ogg"

    log "Creating clean OGG Vorbis Q6..."
    ffmpeg -y -i "$source" -c:a libvorbis -q:a 6 "$output" 2>/dev/null
    log "Created: $output"
}

# 21. AAC from low bitrate source
create_transcode_aac() {
    local source="$1"
    local folder="$2"
    local temp_128="$OUTPUT_DIR/.temp_aac_128.mp3"
    local temp_wav="$OUTPUT_DIR/.temp_aac_decoded.wav"
    local output="$folder/21_FAKE_aac_from_128k.m4a"

    log "Creating FAKE AAC from 128kbps source..."
    lame -b 128 -q 5 "$source" "$temp_128" 2>/dev/null
    ffmpeg -y -i "$temp_128" "$temp_wav" 2>/dev/null
    ffmpeg -y -i "$temp_wav" -c:a aac -b:a 256k "$output" 2>/dev/null

    rm -f "$temp_128" "$temp_wav"
    log "Created: $output"
}

main() {
    echo ""
    echo "╔════════════════════════════════════════════════════════════════════╗"
    echo "║           Losselot Test File Generator                             ║"
    echo "║           Creating demo files for all detection scenarios          ║"
    echo "╚════════════════════════════════════════════════════════════════════╝"
    echo ""

    check_deps

    # Create output directories (multiple folders for collection map demo)
    mkdir -p "$OUTPUT_DIR"
    mkdir -p "$OUTPUT_DIR/Album_Clean"
    mkdir -p "$OUTPUT_DIR/Album_Mixed"
    mkdir -p "$OUTPUT_DIR/Album_Suspect"
    mkdir -p "$OUTPUT_DIR/Lossless_Collection"

    # Clean old files
    rm -f "$OUTPUT_DIR"/*.mp3 "$OUTPUT_DIR"/*.flac "$OUTPUT_DIR"/*.wav "$OUTPUT_DIR"/*.ogg "$OUTPUT_DIR"/*.m4a 2>/dev/null
    rm -f "$OUTPUT_DIR/Album_Clean"/* 2>/dev/null
    rm -f "$OUTPUT_DIR/Album_Mixed"/* 2>/dev/null
    rm -f "$OUTPUT_DIR/Album_Suspect"/* 2>/dev/null
    rm -f "$OUTPUT_DIR/Lossless_Collection"/* 2>/dev/null

    # Generate source material
    SOURCE_WAV="$OUTPUT_DIR/.source.wav"
    generate_source_wav "$SOURCE_WAV" 10

    echo ""
    log "Creating test files in multiple folders..."
    echo ""

    # ════════════════════════════════════════════════════════════════════
    # ROOT FOLDER - Original 9 scenarios
    # ════════════════════════════════════════════════════════════════════
    log "📁 Creating files in root demo_files/..."
    create_clean_320 "$SOURCE_WAV"
    create_clean_v0 "$SOURCE_WAV"
    create_transcode_128_to_320 "$SOURCE_WAV"
    create_multiple_lame_passes "$SOURCE_WAV"
    create_lame_to_ffmpeg_chain "$SOURCE_WAV"
    create_youtube_style "$SOURCE_WAV"
    create_multiple_ffmpeg_passes "$SOURCE_WAV"
    create_borderline_suspect "$SOURCE_WAV"
    create_multi_codec_laundering "$SOURCE_WAV"

    # ════════════════════════════════════════════════════════════════════
    # ALBUM_CLEAN - All legitimate encodes (should show all green)
    # ════════════════════════════════════════════════════════════════════
    echo ""
    log "📁 Creating Album_Clean/ (all legitimate encodes)..."
    create_clean_256 "$SOURCE_WAV" "$OUTPUT_DIR/Album_Clean"
    create_clean_v2 "$SOURCE_WAV" "$OUTPUT_DIR/Album_Clean"
    create_clean_192 "$SOURCE_WAV" "$OUTPUT_DIR/Album_Clean"
    create_clean_128 "$SOURCE_WAV" "$OUTPUT_DIR/Album_Clean"
    create_clean_ogg "$SOURCE_WAV" "$OUTPUT_DIR/Album_Clean"

    # ════════════════════════════════════════════════════════════════════
    # ALBUM_MIXED - Some good, some bad (mixed colors)
    # ════════════════════════════════════════════════════════════════════
    echo ""
    log "📁 Creating Album_Mixed/ (mix of clean and transcoded)..."

    # Clean files
    lame --preset insane -q 0 "$SOURCE_WAV" "$OUTPUT_DIR/Album_Mixed/track01_clean_320.mp3" 2>/dev/null
    log "Created: Album_Mixed/track01_clean_320.mp3"

    lame -V 0 -q 0 "$SOURCE_WAV" "$OUTPUT_DIR/Album_Mixed/track02_clean_v0.mp3" 2>/dev/null
    log "Created: Album_Mixed/track02_clean_v0.mp3"

    # Transcoded files
    TEMP_128="$OUTPUT_DIR/.temp_mix_128.mp3"
    TEMP_WAV="$OUTPUT_DIR/.temp_mix_wav.wav"
    lame -b 128 -q 5 "$SOURCE_WAV" "$TEMP_128" 2>/dev/null
    ffmpeg -y -i "$TEMP_128" "$TEMP_WAV" 2>/dev/null
    lame --preset insane "$TEMP_WAV" "$OUTPUT_DIR/Album_Mixed/track03_FAKE_320.mp3" 2>/dev/null
    log "Created: Album_Mixed/track03_FAKE_320.mp3 (TRANSCODE)"

    lame -b 256 "$TEMP_WAV" "$OUTPUT_DIR/Album_Mixed/track04_FAKE_256.mp3" 2>/dev/null
    log "Created: Album_Mixed/track04_FAKE_256.mp3 (TRANSCODE)"
    rm -f "$TEMP_128" "$TEMP_WAV"

    # Clean again
    lame -b 256 -q 0 "$SOURCE_WAV" "$OUTPUT_DIR/Album_Mixed/track05_clean_256.mp3" 2>/dev/null
    log "Created: Album_Mixed/track05_clean_256.mp3"

    # ════════════════════════════════════════════════════════════════════
    # ALBUM_SUSPECT - All transcoded (should show all red)
    # ════════════════════════════════════════════════════════════════════
    echo ""
    log "📁 Creating Album_Suspect/ (all transcoded - fake collection)..."
    create_transcode_160_to_320 "$SOURCE_WAV" "$OUTPUT_DIR/Album_Suspect"
    create_transcode_flac "$SOURCE_WAV" "$OUTPUT_DIR/Album_Suspect"
    create_transcode_wav "$SOURCE_WAV" "$OUTPUT_DIR/Album_Suspect"
    create_transcode_aac "$SOURCE_WAV" "$OUTPUT_DIR/Album_Suspect"

    # More transcodes
    TEMP_96="$OUTPUT_DIR/.temp_96.mp3"
    TEMP_WAV2="$OUTPUT_DIR/.temp_96_wav.wav"
    lame -b 96 -q 7 "$SOURCE_WAV" "$TEMP_96" 2>/dev/null
    ffmpeg -y -i "$TEMP_96" "$TEMP_WAV2" 2>/dev/null
    lame --preset insane "$TEMP_WAV2" "$OUTPUT_DIR/Album_Suspect/sus_extreme_fake.mp3" 2>/dev/null
    log "Created: Album_Suspect/sus_extreme_fake.mp3 (96kbps → 320kbps)"

    ffmpeg -y -i "$TEMP_WAV2" -c:a flac "$OUTPUT_DIR/Album_Suspect/sus_flac_from_96k.flac" 2>/dev/null
    log "Created: Album_Suspect/sus_flac_from_96k.flac (FLAC from 96kbps)"
    rm -f "$TEMP_96" "$TEMP_WAV2"

    # ════════════════════════════════════════════════════════════════════
    # LOSSLESS_COLLECTION - FLAC and WAV files
    # ════════════════════════════════════════════════════════════════════
    echo ""
    log "📁 Creating Lossless_Collection/ (FLAC/WAV test files)..."
    create_clean_flac "$SOURCE_WAV" "$OUTPUT_DIR/Lossless_Collection"
    create_clean_wav "$SOURCE_WAV" "$OUTPUT_DIR/Lossless_Collection"
    create_flac_from_320 "$SOURCE_WAV" "$OUTPUT_DIR/Lossless_Collection"

    # Additional lossless files
    ffmpeg -y -i "$SOURCE_WAV" -c:a flac -compression_level 8 "$OUTPUT_DIR/Lossless_Collection/hq_flac_level8.flac" 2>/dev/null
    log "Created: Lossless_Collection/hq_flac_level8.flac"

    # Cleanup
    rm -f "$SOURCE_WAV"
    rm -f "$OUTPUT_DIR"/.temp*

    echo ""
    echo "════════════════════════════════════════════════════════════════════"
    log "All test files created!"
    echo ""
    echo "📊 File counts by folder:"
    echo ""
    echo "  demo_files/              $(ls "$OUTPUT_DIR"/*.mp3 2>/dev/null | wc -l | tr -d ' ') files"
    echo "  Album_Clean/             $(ls "$OUTPUT_DIR/Album_Clean"/* 2>/dev/null | wc -l | tr -d ' ') files"
    echo "  Album_Mixed/             $(ls "$OUTPUT_DIR/Album_Mixed"/* 2>/dev/null | wc -l | tr -d ' ') files"
    echo "  Album_Suspect/           $(ls "$OUTPUT_DIR/Album_Suspect"/* 2>/dev/null | wc -l | tr -d ' ') files"
    echo "  Lossless_Collection/     $(ls "$OUTPUT_DIR/Lossless_Collection"/* 2>/dev/null | wc -l | tr -d ' ') files"
    echo ""
    TOTAL=$(find "$OUTPUT_DIR" -type f \( -name "*.mp3" -o -name "*.flac" -o -name "*.wav" -o -name "*.ogg" -o -name "*.m4a" \) | wc -l | tr -d ' ')
    log "Total: $TOTAL audio files across 5 folders"
    echo ""
    echo "Run Losselot to analyze:"
    echo ""
    echo "  ./target/release/losselot $OUTPUT_DIR"
    echo ""
    echo "The Collection Quality Map will show:"
    echo "  🟢 Album_Clean - all green bubbles (legitimate)"
    echo "  🔴 Album_Suspect - all red bubbles (transcoded)"
    echo "  🟡🔴🟢 Album_Mixed - mixed colors"
    echo "  🟢🟡 Lossless_Collection - mostly clean with one suspect"
    echo ""
}

main "$@"
