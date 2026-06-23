use anyhow::Result;
use log::info;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

/// Known SHA-256 checksums for official ggml Whisper models from HuggingFace.
/// If the model is not in this list (custom download), we skip verification.
const MODEL_CHECKSUMS: &[(&str, &str)] = &[
    ("ggml-tiny.en", "c78c86eb1a8faa21b369bcd33207cc90d64ae9df52aa5a0529ca2f58affd8963"),
    ("ggml-base.en", "a03779c86df3323075f5e796cb2ce5029f00b8046c9c5e16b0be2e11d047032c"),
    ("ggml-small.en", "c6138e41004e7fa55e25f58a4e8a1c4a45ed9b5c89d50dea04b1eea0c1503e6b"),
    ("ggml-medium.en", "19e4548ef1c1b5074c4b06e2f5917c88d59a0b96b1148fd4e7e1e0a62e18cc3c"),
];

pub struct SttEngine {
    state: WhisperState,
}

impl SttEngine {
    pub fn new(model_path: &Path) -> Result<Self> {
        if !model_path.exists() {
            info!("Model not found at {:?}. Downloading...", model_path);
            download_model(model_path)?;
        }

        let params = WhisperContextParameters {
            flash_attn: true,
            ..WhisperContextParameters::default()
        };

        // Log which acceleration backend is compiled in.
        if cfg!(feature = "cuda") {
            info!("STT backend: CUDA (GPU-accelerated)");
        } else if cfg!(feature = "metal") {
            info!("STT backend: Metal (GPU-accelerated)");
        } else if cfg!(feature = "vulkan") {
            info!("STT backend: Vulkan (GPU-accelerated)");
        } else {
            info!("STT backend: CPU (build with --features cuda for GPU acceleration)");
        }

        let ctx = WhisperContext::new_with_params(
            model_path.to_str().unwrap(),
            params,
        )?;

        let state = ctx.create_state()?;

        Ok(Self { state })
    }

    pub fn transcribe(
        &mut self,
        samples: &[f32],
        language: &str,
        dictionary: &[String],
        n_threads: u32,
    ) -> Result<String> {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(n_threads as i32);
        params.set_language(Some(language));
        params.set_translate(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        // Format dictionary terms as a natural sentence so Whisper treats
        // them as real vocabulary context rather than a meaningless list.
        if !dictionary.is_empty() {
            let prompt = format_dictionary_prompt(dictionary);
            params.set_initial_prompt(&prompt);
        }

        self.state.full(params, samples)?;

        let n_segments = self.state.full_n_segments()?;
        let mut text = String::new();
        for i in 0..n_segments {
            text.push_str(&self.state.full_get_segment_text(i)?);
        }
        Ok(text.trim().to_string())
    }
}

/// Formats dictionary terms as a natural sentence for Whisper's initial_prompt.
/// This is significantly more effective than a bare comma-separated list because
/// Whisper's decoder conditions on the prompt as if it were real preceding text.
fn format_dictionary_prompt(terms: &[String]) -> String {
    match terms.len() {
        0 => String::new(),
        1 => format!(
            "This dictation may include the term {}.",
            terms[0]
        ),
        2 => format!(
            "This dictation may include terms like {} and {}.",
            terms[0], terms[1]
        ),
        _ => {
            let last = &terms[terms.len() - 1];
            let rest: Vec<&str> = terms[..terms.len() - 1].iter().map(|s| s.as_str()).collect();
            format!(
                "This dictation may include terms like {}, and {}.",
                rest.join(", "),
                last
            )
        }
    }
}

fn download_model(dest: &Path) -> Result<()> {
    let model_name = dest
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid model path"))?;

    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}.bin",
        model_name
    );

    info!("Downloading {} from HuggingFace...", model_name);
    fs::create_dir_all(dest.parent().unwrap())?;

    let response = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(600))
        .call()?;

    // Stream the download with progress reporting.
    let content_length = response
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok());

    let mut reader = response.into_reader();
    let tmp_path = dest.with_extension("bin.part");
    let mut file = fs::File::create(&tmp_path)?;
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

        // Report progress every 10 MB.
        if downloaded - last_report >= 10 * 1024 * 1024 {
            last_report = downloaded;
            if let Some(total) = content_length {
                let pct = (downloaded as f64 / total as f64 * 100.0) as u32;
                info!(
                    "  {} / {} MB ({}%)",
                    downloaded / (1024 * 1024),
                    total / (1024 * 1024),
                    pct
                );
            } else {
                info!("  {} MB downloaded...", downloaded / (1024 * 1024));
            }
        }
    }
    drop(file);

    // Verify checksum if we know the expected hash.
    let hash = format!("{:x}", hasher.finalize());
    if let Some((_name, expected)) = MODEL_CHECKSUMS.iter().find(|(name, _)| *name == model_name) {
        if hash != *expected {
            let _ = fs::remove_file(&tmp_path);
            anyhow::bail!(
                "SHA-256 mismatch for {}! Expected {}, got {}. \
                 The download may be corrupt. Please retry.",
                model_name,
                expected,
                hash
            );
        }
        info!("SHA-256 verified: {}", &hash[..16]);
    }

    // Atomic rename from .part to final path.
    fs::rename(&tmp_path, dest)?;
    info!("Model saved to {:?}", dest);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_dictionary_prompt_empty() {
        assert_eq!(format_dictionary_prompt(&[]), "");
    }

    #[test]
    fn format_dictionary_prompt_single() {
        let terms = vec!["Kubernetes".to_string()];
        let result = format_dictionary_prompt(&terms);
        assert!(result.contains("Kubernetes"));
        assert!(result.contains("dictation"));
    }

    #[test]
    fn format_dictionary_prompt_two() {
        let terms = vec!["Rust".to_string(), "LLVM".to_string()];
        let result = format_dictionary_prompt(&terms);
        assert!(result.contains("Rust"));
        assert!(result.contains("LLVM"));
        assert!(result.contains(" and "));
    }

    #[test]
    fn format_dictionary_prompt_many() {
        let terms = vec![
            "Kubernetes".to_string(),
            "Terraform".to_string(),
            "DALL·E".to_string(),
        ];
        let result = format_dictionary_prompt(&terms);
        assert!(result.contains("Kubernetes"));
        assert!(result.contains(", and DALL·E"));
    }
}
