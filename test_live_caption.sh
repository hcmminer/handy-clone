#!/bin/bash
# Script to test live caption functionality

set -e

echo "🧪 Testing Live Caption..."
echo ""

# Step 1: Rebuild SCK Helper Binary
echo "📦 Step 1: Rebuilding SCK Helper Binary..."
cd src-tauri
xcrun swiftc -o bin/macos-audio-capture \
  src/audio_toolkit/macos_audio_capture.swift \
  -framework ScreenCaptureKit \
  -framework CoreMedia \
  -framework AVFoundation \
  -framework CoreAudio \
  -framework AppKit

if [ $? -eq 0 ]; then
    echo "✅ Binary rebuilt successfully"
else
    echo "❌ Failed to rebuild binary"
    exit 1
fi

# Step 2: Kill old processes
echo ""
echo "🛑 Step 2: Killing old processes..."
pkill -f "macos-audio-capture" 2>/dev/null || echo "   No macos-audio-capture process found"
pkill -f "handy" 2>/dev/null || echo "   No handy process found"
sleep 1

# Step 3: Start app
echo ""
echo "🚀 Step 3: Starting app..."
echo "   App will start in dev mode..."
echo "   Make sure Chrome is playing video with audio!"
echo ""
cd ..
bun tauri dev

