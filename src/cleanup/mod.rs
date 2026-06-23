use crate::config::Config;
use anyhow::Result;

mod candle;
mod command;
mod fast;
mod none;
mod ollama;

pub mod corrections;

pub fn cleanup(text: &str, config: &Config) -> Result<String> {
    let mut result = match config.cleanup_backend.as_str() {
        "none" => none::cleanup(text),
        "command" => command::cleanup(text, config),
        "ollama" => ollama::cleanup(text, config),
        "candle" => candle::cleanup(text, config),
        _ => fast::cleanup(text),
    }?;

    // Apply vocabulary corrections after the main cleanup pass.
    if !config.corrections.is_empty() {
        result = corrections::apply(&result, config);
    }

    Ok(result)
}
