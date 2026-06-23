use crate::config::Config;
use anyhow::Result;
use log::info;
use candle::quantized::gguf_file;
use candle::{Device, Tensor};
use candle_transformers::generation::{LogitsProcessor, Sampling};
use candle_transformers::models::quantized_qwen2::ModelWeights as Qwen2;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use tokenizers::Tokenizer;

static ENGINE: OnceLock<Mutex<Result<CandleEngine, String>>> = OnceLock::new();

struct CandleEngine {
    model: Qwen2,
    tokenizer: Tokenizer,
    device: Device,
}

impl CandleEngine {
    fn new(model_path: &Path, tokenizer_path: &Path) -> Result<Self> {
        let device = Device::Cpu;
        let mut file = std::fs::File::open(model_path)?;
        let model_content = gguf_file::Content::read(&mut file)
            .map_err(|e| anyhow::anyhow!("Failed to read GGUF: {}", e))?;
        let model = Qwen2::from_gguf(model_content, &mut file, &device)
            .map_err(|e| anyhow::anyhow!("Failed to load model: {}", e))?;
        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;
        Ok(Self { model, tokenizer, device })
    }

    fn generate(&mut self, prompt: &str, max_tokens: usize, temperature: f64) -> Result<String> {
        let tokens = self
            .tokenizer
            .encode(prompt, true)
            .map_err(|e| anyhow::anyhow!(e))?;
        let prompt_tokens = tokens.get_ids().to_vec();

        let mut logits_processor = LogitsProcessor::from_sampling(
            42,
            if temperature <= 0.0 {
                Sampling::ArgMax
            } else {
                Sampling::All { temperature }
            },
        );

        // Process full prompt
        let input = Tensor::new(prompt_tokens.as_slice(), &self.device)?.unsqueeze(0)?;
        let logits = self.model.forward(&input, 0)?;
        let logits = logits.squeeze(0)?;
        let mut next_token = logits_processor.sample(&logits)?;

        let mut generated_tokens = vec![next_token];

        let eos_token = self
            .tokenizer
            .get_vocab(true)
            .get("<|im_end|>")
            .copied()
            .unwrap_or(151645);

        // Generate tokens one at a time
        for index in 0..max_tokens {
            let input = Tensor::new(&[next_token], &self.device)?.unsqueeze(0)?;
            let logits = self.model.forward(&input, prompt_tokens.len() + index)?;
            let logits = logits.squeeze(0)?;
            next_token = logits_processor.sample(&logits)?;
            generated_tokens.push(next_token);

            if next_token == eos_token {
                break;
            }
        }

        // Decode generated tokens
        let text = self
            .tokenizer
            .decode(&generated_tokens, true)
            .map_err(|e| anyhow::anyhow!(e))?;

        Ok(text.trim().to_string())
    }
}

pub fn cleanup(text: &str, config: &Config) -> Result<String> {
    let model_path = model_path()?;
    let tokenizer_path = tokenizer_path()?;

    if !model_path.exists() {
        info!("Candle model not found. Downloading (~300MB)...");
        download_verified(&model_path, MODEL_URL, MODEL_SHA256, "candle model")?;
    }
    if !tokenizer_path.exists() {
        info!("Candle tokenizer not found. Downloading...");
        download_verified(&tokenizer_path, TOKENIZER_URL, TOKENIZER_SHA256, "tokenizer")?;
    }

    let engine = ENGINE.get_or_init(|| {
        match CandleEngine::new(&model_path, &tokenizer_path) {
            Ok(e) => Mutex::new(Ok(e)),
            Err(e) => Mutex::new(Err(format!("Candle engine init failed: {}", e))),
        }
    });

    let mut guard = engine.lock().unwrap();
    let engine_ref = guard.as_mut().map_err(|e| anyhow::anyhow!("{}", e))?;

    let prompt = format!(
        "<|im_start|>user\n{}\n\nText:\n{}<|im_end|>\n<|im_start|>assistant\n",
        config.cleanup_prompt, text
    );

    engine_ref.generate(&prompt, 256, 0.1)
}

const MODEL_URL: &str =
    "https://huggingface.co/Qwen/Qwen2-0.5B-Instruct-GGUF/resolve/main/qwen2-0_5b-instruct-q4_0.gguf";
const TOKENIZER_URL: &str =
    "https://huggingface.co/Qwen/Qwen2-0.5B-Instruct/resolve/main/tokenizer.json";

// SHA-256 checksums for integrity verification. Set to None to skip
// verification for files whose upstream hash is not yet pinned.
const MODEL_SHA256: Option<&str> = None;
const TOKENIZER_SHA256: Option<&str> = None;

fn model_path() -> Result<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "flow")
        .ok_or_else(|| anyhow::anyhow!("Could not find data directory"))?;
    Ok(dirs.data_dir().join("llm").join("model.gguf"))
}

fn tokenizer_path() -> Result<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "flow")
        .ok_or_else(|| anyhow::anyhow!("Could not find data directory"))?;
    Ok(dirs.data_dir().join("llm").join("tokenizer.json"))
}

/// Download a file with progress reporting and optional SHA-256 verification.
/// Uses atomic write (download to `.part`, rename on success).
fn download_verified(
    dest: &Path,
    url: &str,
    expected_sha256: Option<&str>,
    label: &str,
) -> Result<()> {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};

    std::fs::create_dir_all(dest.parent().unwrap())?;

    let response = ureq::get(url)
        .timeout(std::time::Duration::from_secs(600))
        .call()?;

    let content_length = response
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok());

    let mut reader = response.into_reader();
    let tmp_path = dest.with_extension("part");
    let mut file = std::fs::File::create(&tmp_path)?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut last_report: u64 = 0;
    let mut buf = [0u8; 65536];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        hasher.update(&buf[..n]);
        downloaded += n as u64;

        // Report progress every 5 MB.
        if downloaded - last_report >= 5 * 1024 * 1024 {
            last_report = downloaded;
            if let Some(total) = content_length {
                let pct = (downloaded as f64 / total as f64 * 100.0) as u32;
                info!(
                    "  {} — {} / {} MB ({}%)",
                    label,
                    downloaded / (1024 * 1024),
                    total / (1024 * 1024),
                    pct
                );
            } else {
                info!("  {} — {} MB downloaded...", label, downloaded / (1024 * 1024));
            }
        }
    }
    drop(file);

    // Verify checksum if we have an expected hash.
    let hash = format!("{:x}", hasher.finalize());
    if let Some(expected) = expected_sha256 {
        if hash != expected {
            let _ = std::fs::remove_file(&tmp_path);
            anyhow::bail!(
                "SHA-256 mismatch for {}! Expected {}, got {}. \
                 The download may be corrupt. Please retry.",
                label,
                expected,
                hash
            );
        }
        info!("{} SHA-256 verified: {}", label, &hash[..16]);
    } else {
        info!("{} downloaded (SHA-256: {})", label, &hash[..16]);
    }

    // Atomic rename from .part to final path.
    std::fs::rename(&tmp_path, dest)?;
    info!("{} saved to {:?}", label, dest);
    Ok(())
}
