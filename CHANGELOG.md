# Changelog

## 0.2.0 — Unreleased

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
- **Install script**: Auto-detects linker (cc/gcc/clang) instead of hardcoding gcc
- **Desktop file**: Added `StartupNotify` and `X-GNOME-Autostart-Enabled`

### Added
- **GPU acceleration**: Optional CUDA, Metal, and Vulkan support via Cargo feature flags (`--features cuda`)
- **Vocabulary correction layer**: Fuzzy post-STT correction dictionary using Jaro-Winkler similarity (≥0.88)
- **Per-project dictionaries**: Auto-loads `.flow-dictionary.toml` from project root (searches 5 levels up)
- **Natural dictionary prompting**: Dictionary terms formatted as natural sentences for better Whisper bias
- **Voice Activity Detection**: Energy-threshold VAD trims leading/trailing silence before transcription
- **Dynamic thread count**: `n_threads` auto-detected from `available_parallelism()`, capped at 16
- **Configurable max recording**: `max_recording_secs` field (default 120s), auto-stops when reached
- **Download progress**: All model downloads (Whisper and Candle) report progress with byte counts
- **SHA-256 verification**: Downloaded models verified against known checksums; corrupt downloads auto-deleted
- **Graceful shutdown**: Clean Ctrl+C handling via signal handler
- **Unknown key warnings**: Config loader warns about unrecognized TOML keys (typo detection)

### Changed
- Test helpers deduplicated from 3 copies to single shared `tests/common/mod.rs`
- Repository URL updated to `santhsecurity/flow`
- `install.sh` passes extra arguments to `cargo build` (e.g. `./install.sh --features cuda`)
- Candle cleanup backend uses structured `log` macros instead of `println!`

### Removed
- Junk `tests/foo/` directory and empty fixture directories

## 0.1.0 — 2026-04-25

### Added
- Push-to-talk voice dictation with global hotkey (`Alt+Shift+D` default)
- Local Whisper.cpp STT with `small.en` model (auto-download)
- Direct text typing at cursor (no clipboard pollution)
- Cross-platform paste: `enigo` on macOS/Windows, `xdotool`/`wtype`/`ydotool` on Linux
- Modular cleanup backends: `fast` (default), `candle`, `ollama`, `command`, `none`
- Native Rust LLM cleanup via Candle (Qwen2 0.5B Instruct GGUF, ~300MB)
- Live stream preview — chunked transcription while recording
- Dictionary bias — Whisper initial prompt for domain-specific terms
- Floating recording overlay on macOS/Windows (tao + softbuffer)
- Desktop notifications on Linux
- TOML configuration with interactive `flow setup` TUI
- Structured logging to `~/.local/share/flow/flow.log`
- Systemd user service for auto-start
- Comprehensive test suite: 78 tests across unit, integration, property, adversarial, and regression
