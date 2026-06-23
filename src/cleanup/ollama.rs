use crate::config::Config;
use anyhow::Result;
use serde::Deserialize;

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

pub fn cleanup(text: &str, config: &Config) -> Result<String> {
    let prompt = format!("{}\n\nText:\n{}", config.cleanup_prompt, text);

    let body = serde_json::json!({
        "model": config.ollama_model,
        "prompt": prompt,
        "stream": false,
        "options": {
            "temperature": 0.1
        }
    });

    let body_str = serde_json::to_string(&body)?;
    let response = ureq::post(&format!("{}/api/generate", config.ollama_url))
        .set("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(120))
        .send_string(&body_str)?;

    let body = response.into_string()?;
    let result: GenerateResponse = serde_json::from_str(&body)?;
    Ok(result.response.trim().to_string())
}
