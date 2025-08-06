# Listen Command

Enter continuous voice interaction mode where you can speak multiple commands without manually triggering recording each time.

## Usage

```bash
/listen [options]
```

## Options

- `--wake-word <word>`: Set a wake word to trigger recording (default: "claude")
- `--max-session <minutes>`: Maximum session duration in minutes (default: 30)
- `--silence-timeout <seconds>`: Timeout for silence detection (default: 2)
- `--verbose`: Show detailed status messages

## Examples

```bash
# Start listening with default settings
/listen

# Listen with custom wake word
/listen --wake-word "computer"

# Limit session to 10 minutes
/listen --max-session 10

# Verbose mode with detailed feedback
/listen --verbose
```

## Usage Instructions

1. **Start the session**: Run `/listen` to enter continuous mode
2. **Wake up**: Say the wake word (default: "claude") to start recording
3. **Give command**: Speak your command after the wake word
4. **Wait for execution**: The command will be processed and executed
5. **Repeat**: Say the wake word again for the next command
6. **Exit**: Say "stop listening" or "exit" to end the session

## Wake Word Examples

- "Claude, show me the README file"
- "Computer, run the tests"
- "Hey Claude, create a new issue"

!bash
#!/bin/bash

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOOKS_DIR="$(dirname "$SCRIPT_DIR")/hooks"

# Default values
WAKE_WORD="claude"
MAX_SESSION_MINUTES=30
SILENCE_TIMEOUT=2
VERBOSE=false
SESSION_ACTIVE=true

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --wake-word)
            WAKE_WORD="$2"
            shift 2
            ;;
        --max-session)
            MAX_SESSION_MINUTES="$2"
            shift 2
            ;;
        --silence-timeout)
            SILENCE_TIMEOUT="$2"
            shift 2
            ;;
        --verbose)
            VERBOSE=true
            shift
            ;;
        --help|-h)
            echo "Usage: /listen [--wake-word <word>] [--max-session <minutes>] [--silence-timeout <seconds>] [--verbose]"
            echo ""
            echo "Enter continuous voice interaction mode"
            echo ""
            echo "Options:"
            echo "  --wake-word      Wake word to trigger recording (default: claude)"
            echo "  --max-session    Maximum session duration in minutes (default: 30)"
            echo "  --silence-timeout Silence detection timeout in seconds (default: 2)"
            echo "  --verbose        Show detailed status messages"
            echo "  --help           Show this help message"
            echo ""
            echo "Commands during session:"
            echo "  Say 'stop listening' or 'exit' to end the session"
            exit 0
            ;;
        *)
            echo "❌ Error: Unknown argument '$1'" >&2
            echo "Use '/listen --help' for usage information" >&2
            exit 1
            ;;
    esac
done

# Check for required environment variables
if [[ -z "$ELEVENLABS_API_KEY" ]]; then
    echo "❌ Error: ELEVENLABS_API_KEY environment variable not set" >&2
    echo "Please add your ElevenLabs API key to your environment" >&2
    exit 1
fi

# Logging function
log() {
    if [[ "$VERBOSE" == "true" ]]; then
        echo "🔊 $1" >&2
    fi
}

# Function to detect wake word in transcription
detect_wake_word() {
    local text="$1"
    local wake_word="$2"
    
    # Convert to lowercase and check if wake word is present
    local text_lower=$(echo "$text" | tr '[:upper:]' '[:lower:]')
    local wake_lower=$(echo "$wake_word" | tr '[:upper:]' '[:lower:]')
    
    if [[ "$text_lower" == *"$wake_lower"* ]]; then
        # Extract command after wake word
        local command_part=$(echo "$text_lower" | sed "s/.*$wake_lower[[:space:]]*//" | sed 's/^[[:space:]]*//')
        echo "$command_part"
        return 0
    fi
    
    return 1
}

# Function to check for exit commands
check_exit_command() {
    local text="$1"
    local text_lower=$(echo "$text" | tr '[:upper:]' '[:lower:]')
    
    if [[ "$text_lower" == *"stop listening"* ]] || [[ "$text_lower" == *"exit"* ]] || [[ "$text_lower" == *"quit"* ]]; then
        return 0
    fi
    
    return 1
}

# Function to listen for wake word
listen_for_wake_word() {
    log "Listening for wake word: '$WAKE_WORD'"
    
    # Record a short clip to check for wake word
    local audio_file
    audio_file=$("$HOOKS_DIR/utils/audio/recorder.py" 3 2>/dev/null)
    
    if [[ $? -ne 0 || -z "$audio_file" ]]; then
        log "Recording failed, retrying..."
        return 1
    fi
    
    # Quick transcription
    local transcription
    transcription=$("$HOOKS_DIR/utils/stt/elevenlabs_stt.py" --file "$audio_file" 2>/dev/null)
    
    # Clean up
    [[ -f "$audio_file" ]] && rm -f "$audio_file"
    
    if [[ $? -ne 0 || -z "$transcription" ]]; then
        log "No speech detected, continuing to listen..."
        return 1
    fi
    
    log "Heard: '$transcription'"
    
    # Check for exit command
    if check_exit_command "$transcription"; then
        echo "👋 Goodbye! Ending listening session." >&2
        SESSION_ACTIVE=false
        return 2
    fi
    
    # Check for wake word and extract command
    local command
    if command=$(detect_wake_word "$transcription" "$WAKE_WORD"); then
        if [[ -n "$command" ]]; then
            echo "🎯 Wake word detected! Command: '$command'" >&2
            echo "$command"
            return 0
        else
            echo "🎯 Wake word detected! Ready for full command..." >&2
            # Record a longer clip for the full command
            record_full_command
            return $?
        fi
    fi
    
    return 1
}

# Function to record full command after wake word
record_full_command() {
    echo "🎙️  Listening for command..." >&2
    
    # Record until silence
    local audio_file
    audio_file=$("$HOOKS_DIR/utils/audio/recorder.py" --auto-stop 2>/dev/null)
    
    if [[ $? -ne 0 || -z "$audio_file" ]]; then
        echo "❌ Recording failed" >&2
        return 1
    fi
    
    # Transcribe the full command
    local transcription
    transcription=$("$HOOKS_DIR/utils/stt/elevenlabs_stt.py" --file "$audio_file" 2>/dev/null)
    
    # Clean up
    [[ -f "$audio_file" ]] && rm -f "$audio_file"
    
    if [[ $? -ne 0 || -z "$transcription" ]]; then
        echo "❌ No command detected" >&2
        return 1
    fi
    
    echo "$transcription"
    return 0
}

# Function to process and execute command
execute_voice_command() {
    local command="$1"
    
    echo "🎯 Processing: \"$command\"" >&2
    
    # Preprocess the command
    local processed_command
    processed_command=$("$HOOKS_DIR/utils/voice/preprocessor.py" --validate --suggestions "$command" 2>&1)
    local preprocess_exit=$?
    
    if [[ $preprocess_exit -ne 0 ]]; then
        echo "❌ Command validation failed" >&2
        echo "$processed_command" >&2
        return 1
    fi
    
    # Extract the processed command (last line of output)
    local final_command
    final_command=$(echo "$processed_command" | tail -n 1)
    
    if [[ "$command" != "$final_command" ]]; then
        echo "🔄 Processed: \"$final_command\"" >&2
    fi
    
    echo "▶️  Executing: $final_command" >&2
    echo ""
    
    # Execute the command by outputting it
    echo "$final_command"
}

# Trap to handle Ctrl+C gracefully
trap 'echo -e "\n👋 Session interrupted. Goodbye!" >&2; exit 0' INT

# Main listening loop
main() {
    echo "🎧 Starting continuous voice interaction mode" >&2
    echo "👂 Wake word: '$WAKE_WORD'" >&2
    echo "⏰ Session timeout: $MAX_SESSION_MINUTES minutes" >&2
    echo "🔇 Silence timeout: $SILENCE_TIMEOUT seconds" >&2
    echo "🛑 Say 'stop listening' or 'exit' to end session" >&2
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" >&2
    
    local session_start=$(date +%s)
    local max_session_seconds=$((MAX_SESSION_MINUTES * 60))
    
    while [[ "$SESSION_ACTIVE" == "true" ]]; do
        # Check session timeout
        local current_time=$(date +%s)
        local elapsed=$((current_time - session_start))
        
        if [[ $elapsed -ge $max_session_seconds ]]; then
            echo "⏰ Session timeout reached. Ending listening session." >&2
            break
        fi
        
        # Listen for wake word or command
        local command
        command=$(listen_for_wake_word)
        local listen_result=$?
        
        case $listen_result in
            0)
                # Command detected, execute it
                if [[ -n "$command" ]]; then
                    execute_voice_command "$command"
                    echo "" >&2
                    echo "👂 Listening for next command..." >&2
                fi
                ;;
            2)
                # Exit command detected
                break
                ;;
            1)
                # No wake word or command, continue listening
                sleep 0.5
                ;;
        esac
    done
    
    echo "✅ Voice interaction session ended." >&2
}

# Run main function
main