# Mist

Local-first voice dictation daemon for Linux, macOS, and Windows.

Push-to-talk → transcribe → type. No cloud. No clipboard. Just your voice at your cursor.

## Screenshots

The overlay follows your cursor and shows the real waveform plus the live / final text.

| Listening | Processing | Done |
|---|---|---|
| ![listening](assets/screenshots/listening.png) | ![processing](assets/screenshots/processing.png) | ![done](assets/screenshots/done.png) |

You can regenerate these with `mist screenshot`.

## Features

- **Zero external setup** — Whisper runs locally; optional Candle LLM cleanup
- **Direct typing** — Text appears at your cursor, no clipboard pollution
- **Cross-platform** — Linux (X11/Wayland), macOS, Windows
- **Hold-to-talk** — Hold hotkey to record, release to transcribe and type
- **Cursor-following overlay** — A minimal, real-time waveform and live text preview appears near your cursor while you speak
- **Premium rendering** — Anti-aliased capsule, hairline border, smooth waveform, system typography
- **Modular cleanup** — `fast` (default), `candle`, `ollama`, `command`, or `none`
- **Vocabulary correction** — Fuzzy post-STT correction dictionary for domain terms
- **Phrase replacements** — Expand shortcuts like "my email" → `you@example.com`
- **Dictionary bias** — Natural-sentence Whisper prompting for custom vocabulary
- **Per-project dictionaries** — Auto-load terms, corrections, and replacements from `.mist-dictionary.toml`
- **Voice Activity Detection** — Trims silence before transcription for faster results
- **Live preview** — Optional chunked transcription while you speak
- **Graceful shutdown** — Clean Ctrl+C handling, systemd integration
- **Health check** — `mist status` shows config, model, and typing-backend state

## Install

```bash
./install.sh
```

`install.sh` will:

1. Auto-detect Apple Silicon (CoreML) or OpenBLAS and enable the matching acceleration feature.
2. Check for a typing tool on Linux (`xdotool`, `wtype`, `ydotool`) and warn if missing.
3. Optionally download the default Whisper model.
4. Run `mist setup` interactively.
5. Install the systemd user service and desktop entry.

Or manually:

```bash
cargo build --release
# Binary: target/release/mist
```

### GPU / NPU Acceleration

Mist supports accelerated speech recognition via `whisper-rs` feature flags. CPU mode works out of the box.

| Feature | Requires | Speedup |
|---------|----------|---------|
| `cuda` | NVIDIA GPU + CUDA Toolkit | 10-50× |
| `metal` | Apple Silicon / macOS | 5-20× |
| `coreml` | Apple Silicon / macOS | 2-5× on Neural Engine |
| `vulkan` | Vulkan SDK | 5-15× |
| `openblas` | OpenBLAS dev libs | 1.5-3× on CPU |

```bash
# NVIDIA GPU
cargo build --release --features cuda

# Apple Silicon (Metal + CoreML)
cargo build --release --features "metal coreml"

# Apple Silicon Neural Engine only
cargo build --release --features coreml

# Vulkan (cross-platform)
cargo build --release --features vulkan

# CPU with BLAS on Linux
sudo apt install libopenblas-dev
cargo build --release --features openblas
```

### Systemd (Linux)

```bash
systemctl --user enable mist
systemctl --user start mist
systemctl --user status mist
```

## Usage

```bash
mist              # Run daemon (default)
mist run          # Explicitly run daemon
mist setup        # Interactive configuration
mist status       # Show daemon status
mist screenshot   # Generate overlay screenshots
mist dictionary add Kubernetes
mist dictionary list
mist dictionary import ./my-dict.toml
mist dictionary export ./my-dict.toml
```

**Default hotkey:** `Alt+Shift+D`

Hold to record, release to transcribe and type. Second press also stops (fallback toggle mode).

## Configuration

Config lives at `~/.config/mist/config.toml` (auto-created on first run).

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

[[replacements]]
pattern = "my email"
replacement = "you@example.com"
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

Create `.mist-dictionary.toml` in your project root:

```toml
terms = ["Kubernetes", "Terraform", "gRPC"]

[[corrections]]
patterns = ["kuber netties"]
correct = "Kubernetes"

[[replacements]]
pattern = "my email"
replacement = "you@example.com"
```

Mist walks up 5 parent directories looking for this file and merges the entries with your global config.

### Vocabulary corrections

The `[[corrections]]` table maps common Whisper misrecognitions to their correct spelling using fuzzy matching (Jaro-Winkler similarity ≥ 0.88). This runs after transcription in <1ms and is 100% deterministic.

### Phrase replacements

The `[[replacements]]` table expands shortcuts after cleanup. Patterns are matched case-insensitively as whole phrases.

## Models

Whisper models auto-download on first run to `~/.local/share/mist/models/`:

| Model | Size | English-only | Notes |
|-------|------|-------------|-------|
| `tiny.en` | ~75MB | ✓ | Fastest, lowest accuracy |
| `base.en` | ~142MB | ✓ | Good balance |
| `small.en` | ~466MB | ✓ | Default, high accuracy |
| `small.en-q5_0` | ~180MB | ✓ | Quantized; faster, nearly same accuracy |
| `medium.en` | ~1.5GB | ✓ | Highest accuracy CPU model |
| `medium.en-q5_0` | ~550MB | ✓ | Quantized medium |
| `large-v3-turbo-q5_0` | ~900MB | — | Turbo large; fastest large-class model |

Downloads include progress reporting and SHA-256 verification for known models.

## Logs

Logs write to `~/.local/share/mist/mist.log` by default (auto-rotated at 10 MB). Set `RUST_LOG=debug` for verbose output to stderr.

## Test

```bash
cargo test
```

## Architecture

```
main.rs        → tao event loop + hotkey + graceful shutdown
audio.rs       → cpal capture + VAD + bounded buffer
stt.rs         → whisper-rs + natural dictionary prompting
cleanup/       → pluggable backends + corrections + replacements
paste.rs       → direct typing (xdotool/wtype/ydotool/enigo) with caching
overlay.rs     → cursor-following overlay with real waveform + text
config.rs      → TOML + interactive setup + per-project dict + CLI dict edits
hotkey.rs      → global-hotkey parsing
```

## License

MIT
