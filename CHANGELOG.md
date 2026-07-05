# Changelog

## 0.2.0 - Unreleased

### Fixed
- **CPU burn**: Event loop now sleeps 16ms between iterations instead of busy-polling
- **Hold-to-release**: Hotkey now properly handles `Released` events for true hold-to-talk; second press still toggles as fallback
- **Unbounded audio buffer**: Recordings are capped at `max_recording_secs` (default 120s) to prevent OOM
- **Candle crash**: OnceLock no longer panics if the Candle model fails to load; error is propagated cleanly
- **U16 sample conversion**: Correct mapping of [0, 65535] → [-1.0, 1.0] in audio capture
- **Mixed logging**: All `eprintln!()` calls replaced with structured `error!()`/`warn!()` to log file
- **Log rotation**: Log file auto-rotates at 10 MB (renames to `.log.old`)
- **Linux paste latency**: Typing backend (xdotool/wtype/ydotool) detected once at startup instead of every paste
- **Wayland detection**: Paste module now checks `$XDG_SESSION_TYPE` and skips xdotool on Wayland
- **Fast cleanup**: Rewritten with compiled regex for case-insensitive filler removal; no longer removes legitimate "like"
- **Hotkey conflict UX**: Clear error message and example alternatives when the shortcut cannot be registered
- **Install script**: Auto-detects linker (cc/gcc/clang) instead of hardcoding gcc
- **Desktop file**: Added `StartupNotify` and `X-GNOME-Autostart-Enabled`

### Added
- **Cursor-following overlay**: Minimal, monochrome status bar positioned near the mouse cursor, tracking it while visible
- **System font rendering**: Overlay text drawn with `fontdb` + `fontdue`, falling back to iconic mode if no font is found
- **GPU acceleration**: Optional CUDA, Metal, Vulkan, CoreML, and OpenBLAS support via Cargo feature flags
- **More models**: Quantized `small.en-q5_0`, `medium.en-q5_0`, and `large-v3-turbo-q5_0` options for faster inference
- **Phrase replacements**: Shortcut expansion via `[[replacements]]` config table (e.g. "my email" → `you@example.com`)
- **Richer per-project dictionaries**: `.mist-dictionary.toml` now supports `terms`, `[[corrections]]`, and `[[replacements]]`
- **CLI dictionary management**: `mist dictionary add|remove|list|import|export`
- **`mist status` command**: Prints config, model, data dir, and typing-backend state
- **`mist logs` command**: Prints the latest 200 lines of the daemon log
- **`mist doctor` command**: Runs environment diagnostics (config, model, mic, typing backend)
- **`mist model` command**: `list`, `download`, and `remove` Whisper models
- **Vocabulary correction layer**: Fuzzy post-STT correction dictionary using Jaro-Winkler similarity (>=0.88)
- **Per-project dictionaries**: Auto-loads `.mist-dictionary.toml` from project root (searches 5 levels up)
- **Natural dictionary prompting**: Dictionary terms formatted as natural sentences for better Whisper bias
- **Voice Activity Detection**: Energy-threshold VAD trims leading/trailing silence before transcription
- **Toggle mode**: Press the hotkey once to start and again to stop instead of holding
- **Audio feedback**: Optional start/stop clicks via the default output device
- **System tray icon**: Menu with open config/data folder and quit actions
- **Per-project dictionary live reload**: Edits to `.mist-dictionary.toml` are picked up on the next dictation
- **Dynamic thread count**: `n_threads` auto-detected from `available_parallelism()`, capped at 16
- **Configurable max recording**: `max_recording_secs` field (default 120s), auto-stops when reached
- **Download progress**: All model downloads (Whisper and Candle) report progress with byte counts
- **SHA-256 verification**: Downloaded models verified against known checksums; corrupt downloads auto-deleted
- **Graceful shutdown**: Clean Ctrl+C handling via signal handler
- **Unknown key warnings**: Config loader warns about unrecognized TOML keys (typo detection)

### Changed
- Project and binary renamed from `flow` to `mist`; config/data directories, systemd service, desktop entry, and per-project dictionary file (`.mist-dictionary.toml`) updated accordingly
- `install.sh` is now guided: auto-detects acceleration, checks typing tools, optionally downloads the default model, and runs `mist setup`
- Overlay is now a monochrome text-only bar; waveform removed in favor of cleaner state labels
- Test helpers deduplicated from 3 copies to single shared `tests/common/mod.rs`
- Repository URL updated to `santhsecurity/mist`
- `install.sh` passes extra arguments to `cargo build` (e.g. `./install.sh --features cuda`)
- Candle cleanup backend uses structured `log` macros instead of `println!`

### Fixed
- Audio mutex poisoning is now recovered instead of panicking
- Detects microphone permission/mute issues after ~1s of recording and surfaces an actionable notification
- Surfaces a warning notification when no Linux typing backend is available

### Removed
- Junk `tests/foo/` directory and empty fixture directories

## 0.1.0 - 2026-04-25

### Added
- Push-to-talk voice dictation with global hotkey (`Alt+Shift+D` default)
- Local Whisper.cpp STT with `small.en` model (auto-download)
- Direct text typing at cursor (no clipboard pollution)
- Cross-platform paste: `enigo` on macOS/Windows, `xdotool`/`wtype`/`ydotool` on Linux
- Modular cleanup backends: `fast` (default), `candle`, `ollama`, `command`, `none`
- Native Rust LLM cleanup via Candle (Qwen2 0.5B Instruct GGUF, ~300MB)
- Live stream preview - chunked transcription while recording
- Dictionary bias - Whisper initial prompt for domain-specific terms
- Floating recording overlay on macOS/Windows (tao + softbuffer)
- Desktop notifications on Linux
- TOML configuration with interactive `mist setup` TUI
- Structured logging to `~/.local/share/mist/mist.log`
- Systemd user service for auto-start
- Comprehensive test suite: 78 tests across unit, integration, property, adversarial, and regression
