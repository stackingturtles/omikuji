# Voice Commands for Claude Code

This system enables voice control of Claude Code using ElevenLabs speech-to-text API. You can speak commands instead of typing them, making coding more accessible and hands-free.

## 🎯 Features

- **One-shot voice commands** with `/voice`
- **Continuous listening mode** with `/listen`
- **Intelligent preprocessing** to convert natural speech to commands
- **Safety validation** to prevent dangerous commands
- **Cross-platform audio recording** with visual feedback
- **Configurable settings** for personalization

## 🚀 Quick Start

### 1. Prerequisites

Ensure you have your ElevenLabs API key set in your environment:

```bash
export ELEVENLABS_API_KEY="your_api_key_here"
```

### 2. Basic Usage

```bash
# Record for 5 seconds and execute the transcribed command
/voice

# Record for 10 seconds
/voice 10

# Transcribe an existing audio file
/voice --file audio.wav
```

### 3. Continuous Mode

```bash
# Start continuous listening
/listen

# Then speak: "Claude, show me the README file"
# Or: "Claude, run the tests"
# Say "stop listening" to exit
```

## 📖 Commands Reference

### `/voice` - One-Shot Voice Commands

Record audio and execute the transcribed command immediately.

**Syntax:**
```bash
/voice [duration] [--auto-stop] [--file <path>]
```

**Options:**
- `duration`: Recording duration in seconds (default: 5)
- `--auto-stop`: Stop recording when silence is detected
- `--file <path>`: Transcribe an existing audio file

**Examples:**
```bash
/voice                          # Record for 5 seconds
/voice 10                       # Record for 10 seconds  
/voice --auto-stop              # Record until silence
/voice --file recording.wav     # Transcribe existing file
```

### `/listen` - Continuous Voice Interaction

Enter a continuous listening mode where you can speak multiple commands.

**Syntax:**
```bash
/listen [--wake-word <word>] [--max-session <minutes>] [--verbose]
```

**Options:**
- `--wake-word <word>`: Set custom wake word (default: "claude")
- `--max-session <minutes>`: Maximum session duration (default: 30)
- `--silence-timeout <seconds>`: Silence detection timeout (default: 2)
- `--verbose`: Show detailed status messages

**Examples:**
```bash
/listen                              # Start with default settings
/listen --wake-word "computer"       # Use "computer" as wake word
/listen --max-session 10             # Limit to 10 minutes
/listen --verbose                    # Show detailed feedback
```

**Usage in listening mode:**
1. Start with `/listen`
2. Say the wake word ("claude" by default)
3. Speak your command
4. Wait for execution
5. Repeat for more commands
6. Say "stop listening" or "exit" to end

## 🎙️ Voice Command Examples

### Natural Language Examples

These natural phrases will be automatically converted to proper commands:

| What you say | What gets executed |
|--------------|------------------|
| "Please show me the README file" | `/read` |
| "Could you run the tests" | `cargo test` |
| "Create a new issue" | `/issues` |
| "Let me see the open issues" | `/open-issues` |
| "Make a commit" | `/commit` |
| "List all files" | `ls` |
| "Show me the git status" | `git status` |

### Direct Commands

You can also speak commands directly:

| Voice input | Executed command |
|-------------|-----------------|
| "git status" | `git status` |
| "cargo build" | `cargo build` |
| "slash read" | `/read` |
| "npm test" | `npm test` |

## ⚙️ Configuration

### Voice Settings

Configure voice behavior by editing `.claude/hooks/utils/voice/voice_config.yaml`:

```yaml
audio:
  sample_rate: 16000      # Audio quality
  channels: 1             # Mono/stereo
  
recording:
  default_duration: 5     # Default recording time
  silence_threshold: 0.01 # Sensitivity for silence detection
  
listening:
  wake_word: "claude"     # Wake word for continuous mode
  max_session_minutes: 30 # Session timeout
  
feedback:
  show_transcription: true     # Display what was heard
  show_preprocessing: true     # Show command transformations
```

### Configuration Management

```bash
# Show current configuration
python .claude/hooks/utils/voice/config.py --show

# Validate configuration
python .claude/hooks/utils/voice/config.py --validate

# Get a specific setting
python .claude/hooks/utils/voice/config.py --get audio.sample_rate

# Set a specific setting
python .claude/hooks/utils/voice/config.py --set listening.wake_word "computer"

# Create default configuration file
python .claude/hooks/utils/voice/config.py --create-default
```

## 🔧 Technical Details

### System Architecture

```
Voice Input → Audio Recording → ElevenLabs STT → Preprocessing → Command Execution
                     ↓
               Visual Feedback
```

### Components

1. **Audio Recording** (`utils/audio/recorder.py`)
   - Cross-platform microphone access
   - Real-time progress feedback
   - Silence detection

2. **Speech-to-Text** (`utils/stt/elevenlabs_stt.py`)
   - ElevenLabs Scribe v1 model
   - High-quality transcription
   - Multi-language support

3. **Preprocessing** (`utils/voice/preprocessor.py`)
   - Natural language to command conversion
   - Safety validation
   - Command suggestions

4. **Configuration** (`utils/voice/config.py`)
   - YAML-based settings
   - Runtime configuration management
   - Validation and defaults

### Audio Requirements

- **Microphone**: Any system microphone
- **Sample Rate**: 16kHz (recommended for STT accuracy)
- **Format**: 16-bit mono PCM
- **Latency**: ~2-3 seconds for transcription

### Dependencies

The voice system automatically manages dependencies using UV:

- `elevenlabs`: ElevenLabs API client
- `pyaudio`: Cross-platform audio recording
- `numpy`: Audio processing
- `pyyaml`: Configuration management

## 🛠️ Troubleshooting

### Common Issues

#### "PyAudio not available"
Install system audio dependencies:
```bash
# macOS
brew install portaudio

# Ubuntu/Debian
sudo apt-get install portaudio19-dev

# Windows
# PyAudio should work out of the box
```

#### "ELEVENLABS_API_KEY not found"
Set your API key:
```bash
export ELEVENLABS_API_KEY="your_api_key_here"
# Or add to your .env file
echo "ELEVENLABS_API_KEY=your_api_key_here" >> .env
```

#### "No speech detected"
- Check microphone permissions
- Increase recording duration
- Reduce background noise
- Speak closer to microphone

#### "Command validation failed"
- Check the preprocessing output
- Verify command syntax
- Use simpler, more direct language

### Debug Mode

Enable verbose logging for troubleshooting:

```bash
# Verbose voice command
/voice --help  # Shows all options

# Verbose listening mode
/listen --verbose

# Test preprocessing
python .claude/hooks/utils/voice/preprocessor.py --suggestions "show me the readme"
```

### Testing Components

Test individual components:

```bash
# Test audio recording
python .claude/hooks/utils/audio/recorder.py 5

# Test speech-to-text with file
python .claude/hooks/utils/stt/elevenlabs_stt.py --file test.wav

# Test preprocessing
echo "please show me the readme file" | python .claude/hooks/utils/voice/preprocessor.py --stdin --suggestions
```

## 🔐 Security & Privacy

### Safety Features

- **Command validation**: Prevents dangerous operations
- **Restricted commands**: Blocks potentially harmful patterns
- **User confirmation**: Shows what will be executed before running

### Privacy Considerations

- **Local processing**: Audio is recorded locally
- **API transmission**: Audio sent to ElevenLabs for transcription
- **No storage**: Temporary files are automatically cleaned up
- **Configuration**: Disable logging in ElevenLabs API settings if needed

### Safe Usage

- Always review transcribed commands before execution
- Use in trusted environments
- Keep API keys secure
- Be aware of background conversations

## 📚 Examples

### Complete Workflow Example

```bash
# Start Claude Code
claude-code

# Start continuous listening
/listen

# Voice commands (speak these):
"Claude, show me the readme file"
# → Executes: /read

"Claude, run cargo test"  
# → Executes: cargo test

"Claude, create a commit"
# → Executes: /commit

"Stop listening"
# → Exits listening mode
```

### One-Shot Command Example

```bash
# Quick voice command
/voice 8
# Speak: "show me all open issues"
# → Executes: /open-issues
```

This voice command system transforms Claude Code into a hands-free coding assistant, making development more accessible and efficient!