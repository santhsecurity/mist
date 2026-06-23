# Flow

Local-first voice dictation daemon for Linux, macOS, and Windows.

Push-to-talk → transcribe → type. No cloud. No clipboard. Just your voice at your cursor.

## Features

- **Zero external setup** — Whisper runs locally; optional Candle LLM cleanup
- **Direct typing** — Text appears at your cursor, no clipboard pollution
- **Cross-platform** — Linux (X11/Wayland), macOS, Windows
- **Hold-to-talk** — Hold hotkey to record, release to transcribe and type
- **Minimal UI** — Floating overlay while recording (macOS/Windows), notifications on Linux
- **Modular cleanup** — `fast` (default), `candle`, `ollama`, `command`, or `none`
- **Vocabulary correction** — Fuzzy post-STT correction dictionary for domain terms
- **Dictionary bias** — Natural-sentence Whisper prompting for custom vocabulary
- **Per-project dictionaries** — Auto-load terms from `.flow-dictionary.toml` in your project
- **Voice Activity Detection** — Trims silence before transcription for faster results
- **Live preview** — Optional chunked transcription while you speak
- **Graceful shutdown** — Clean Ctrl+C handling, systemd integration

## Install

```bash
./install.sh
```

Or manually:

```bash
cargo build --release
# Binary: target/release/flow
```

### GPU Acceleration

Flow supports GPU-accelerated speech recognition via `whisper-rs` feature flags. This is optional — CPU mode works out of the box.

| Feature | Requires | Speedup |
|---------|----------|---------|
| `cuda` | NVIDIA GPU + CUDA Toolkit | 10-50× |
| `metal` | Apple Silicon / macOS | 5-20× |
| `vulkan` | Vulkan SDK | 5-15× |

```bash
# NVIDIA GPU
cargo build --release --features cuda

# Apple Silicon
cargo build --release --features metal

# Vulkan (cross-platform)
cargo build --release --features vulkan
```

### Systemd (Linux)

```bash
systemctl --user enable flow
systemctl --user start flow
systemctl --user status flow
```

## Usage

```bash
flow              # Run daemon (default)
flow run          # Explicitly run daemon
flow setup        # Interactive configuration
```

**Default hotkey:** `Alt+Shift+D`

Hold to record, release to transcribe and type. Second press also stops (fallback toggle mode).

## Configuration

Config lives at `~/.config/flow/config.toml` (auto-created on first run).

```toml
hotkey = "Alt+Shift+D"
model = "small.en"
language = "en"
cleanup_backend = "fast"
cleanup_enabled = true
live_stream = false
show_overlay = true
max_recording_secs = 120
n_threads = 0                          # 0 = auto-detect
ollama_model = "qwen3:0.6b"
ollama_url = "http://localhost:11434"
cleanup_prompt = "Clean up this text."
dictionary = ["Rust", "LLM"]

[[corrections]]
patterns = ["kubernetes", "kuber netties", "cooper nettys"]
correct = "Kubernetes"

[[corrections]]
patterns = ["dall-e", "dolly"]
correct = "DALL·E"
```

### Cleanup backends

| Backend | Description | Requires |
|---------|-------------|----------|
| `fast` | Regex filler removal, zero latency | Nothing |
| `candle` | Native Qwen2 0.5B GGUF | ~300MB download |
| `ollama` | HTTP to local Ollama | Ollama running |
| `command` | Shell command stdin/stdout | Your tool |
| `none` | Passthrough | Nothing |

### Per-project dictionary

Create `.flow-dictionary.toml` in your project root:

```toml
terms = ["Kubernetes", "Terraform", "gRPC"]
```

Flow walks up 5 parent directories looking for this file and merges the terms with your global dictionary.

### Vocabulary corrections

The `[[corrections]]` table maps common Whisper misrecognitions to their correct spelling using fuzzy matching (Jaro-Winkler similarity ≥ 0.88). This runs after transcription in <1ms and is 100% deterministic.

## Models

Whisper models auto-download on first run to `~/.local/share/flow/models/`:

| Model | Size | English-only |
|-------|------|-------------|
| `tiny.en` | ~75MB | ✓ |
| `base.en` | ~142MB | ✓ |
| `small.en` | ~466MB | ✓ |
| `medium.en` | ~1.5GB | ✓ |

Downloads include progress reporting and SHA-256 verification.

## Logs

Logs write to `~/.local/share/flow/flow.log` by default (auto-rotated at 10 MB). Set `RUST_LOG=debug` for verbose output to stderr.

## Test

```bash
cargo test
```

## Architecture

```
main.rs        → tao event loop + hotkey + graceful shutdown
audio.rs       → cpal capture + VAD + bounded buffer
stt.rs         → whisper-rs + natural dictionary prompting
cleanup/       → pluggable backends + corrections layer
paste.rs       → direct typing (xdotool/wtype/ydotool) with caching
overlay.rs     → tao + softbuffer (macOS/Windows)
config.rs      → TOML + interactive setup + per-project dict
hotkey.rs      → global-hotkey parsing
```

## License

MIT
