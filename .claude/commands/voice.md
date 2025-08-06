# Voice Command

Record audio from your microphone and convert it to text using ElevenLabs speech-to-text, then execute the transcribed command as if you typed it.

## Usage

```bash
/voice [duration] [options]
```

## Arguments

- `duration` (optional): Recording duration in seconds (default: 5)
- `--auto-stop`: Stop recording automatically when silence is detected
- `--file <path>`: Transcribe an existing audio file instead of recording

## Examples

```bash
# Record for 5 seconds (default)
/voice

# Record for 10 seconds
/voice 10

# Record until silence detected (up to 30 seconds)
/voice --auto-stop

# Transcribe an existing audio file
/voice --file /path/to/audio.wav
```

## Implementation

This command uses the ElevenLabs speech-to-text API to transcribe your voice input and then executes the resulting text as a Claude Code command.

!bash
#!/bin/bash

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOOKS_DIR="$(dirname "$SCRIPT_DIR")/hooks"

# Default values
DURATION=5
AUTO_STOP=false
AUDIO_FILE=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --auto-stop)
            AUTO_STOP=true
            shift
            ;;
        --file)
            AUDIO_FILE="$2"
            shift 2
            ;;
        --help|-h)
            echo "Usage: /voice [duration] [--auto-stop] [--file <path>]"
            echo ""
            echo "Record and transcribe voice input, then execute as command"
            echo ""
            echo "Options:"
            echo "  duration     Recording duration in seconds (default: 5)"
            echo "  --auto-stop  Stop when silence detected"
            echo "  --file       Transcribe existing audio file"
            echo "  --help       Show this help message"
            exit 0
            ;;
        *)
            if [[ "$1" =~ ^[0-9]+$ ]]; then
                DURATION="$1"
            else
                echo "❌ Error: Unknown argument '$1'" >&2
                echo "Use '/voice --help' for usage information" >&2
                exit 1
            fi
            shift
            ;;
    esac
done

# Check for required environment variables
if [[ -z "$ELEVENLABS_API_KEY" ]]; then
    echo "❌ Error: ELEVENLABS_API_KEY environment variable not set" >&2
    echo "Please add your ElevenLabs API key to your environment" >&2
    exit 1
fi

# Function to transcribe audio file
transcribe_audio() {
    local audio_file="$1"
    
    echo "🤖 Transcribing audio..." >&2
    
    # Call the ElevenLabs STT script
    local transcription
    transcription=$("$HOOKS_DIR/utils/stt/elevenlabs_stt.py" --file "$audio_file" 2>/dev/null)
    local exit_code=$?
    
    if [[ $exit_code -ne 0 ]]; then
        echo "❌ Transcription failed" >&2
        return 1
    fi
    
    if [[ -z "$transcription" ]]; then
        echo "❌ No speech detected in audio" >&2
        return 1
    fi
    
    echo "$transcription"
    return 0
}

# Function to record and transcribe
record_and_transcribe() {
    local duration="$1"
    local auto_stop="$2"
    
    echo "🎙️  Starting voice recording..." >&2
    
    # Record audio
    local audio_file
    if [[ "$auto_stop" == "true" ]]; then
        audio_file=$("$HOOKS_DIR/utils/audio/recorder.py" --auto-stop)
    else
        audio_file=$("$HOOKS_DIR/utils/audio/recorder.py" "$duration")
    fi
    
    if [[ $? -ne 0 || -z "$audio_file" ]]; then
        echo "❌ Recording failed" >&2
        return 1
    fi
    
    # Transcribe the recorded audio
    local transcription
    transcription=$(transcribe_audio "$audio_file")
    local exit_code=$?
    
    # Clean up temporary file
    [[ -f "$audio_file" ]] && rm -f "$audio_file"
    
    if [[ $exit_code -ne 0 ]]; then
        return 1
    fi
    
    echo "$transcription"
    return 0
}

# Main execution
main() {
    local transcription
    
    if [[ -n "$AUDIO_FILE" ]]; then
        # Transcribe existing file
        if [[ ! -f "$AUDIO_FILE" ]]; then
            echo "❌ Error: Audio file not found: $AUDIO_FILE" >&2
            exit 1
        fi
        
        transcription=$(transcribe_audio "$AUDIO_FILE")
    else
        # Record and transcribe
        transcription=$(record_and_transcribe "$DURATION" "$AUTO_STOP")
    fi
    
    if [[ $? -ne 0 ]]; then
        exit 1
    fi
    
    # Clean up the transcription
    transcription=$(echo "$transcription" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
    
    if [[ -z "$transcription" ]]; then
        echo "❌ No valid transcription received" >&2
        exit 1
    fi
    
    echo "🎯 Transcribed: \"$transcription\"" >&2
    
    # Preprocess the transcription for better command recognition
    local processed_command
    processed_command=$("$HOOKS_DIR/utils/voice/preprocessor.py" --validate --suggestions "$transcription" 2>&1)
    local preprocess_exit=$?
    
    if [[ $preprocess_exit -ne 0 ]]; then
        echo "❌ Command validation failed" >&2
        echo "$processed_command" >&2
        exit 1
    fi
    
    # Extract the processed command (last line of output)
    local final_command
    final_command=$(echo "$processed_command" | tail -n 1)
    
    if [[ "$transcription" != "$final_command" ]]; then
        echo "🔄 Processed: \"$final_command\"" >&2
    fi
    
    echo "▶️  Executing command..." >&2
    echo ""
    
    # Output the processed command - Claude Code will treat this as user input
    echo "$final_command"
}

# Run main function
main