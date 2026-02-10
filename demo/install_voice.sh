#!/bin/bash
set -e

echo "🐟 Installing Fish Audio SDK for SOTA TTS..."
pip3 install fish-audio-sdk soundfile numpy

echo "✅ Installed! You can now use fish-audio dependent scripts."
# Example usage:
# python3 -c "from fish_audio_sdk import Session; session = Session('YOUR_API_KEY'); session.tts(text='Hello world', reference_id='...')"
