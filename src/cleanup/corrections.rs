//! Post-transcription vocabulary correction layer.
//!
//! Applies deterministic fuzzy corrections to the transcribed text based on
//! user-defined correction entries in the config. This catches Whisper's most
//! common misrecognitions for domain-specific terms and runs in <1ms.

use crate::config::Config;
use strsim::jaro_winkler;

/// Apply all vocabulary corrections to the text.
///
/// For each word in the text, checks if it matches any correction pattern
/// either exactly (case-insensitive) or via fuzzy matching (Jaro-Winkler
/// similarity ≥ 0.88). Replaces the word with the canonical spelling.
#[must_use]
pub fn apply(text: &str, config: &Config) -> String {
    if config.corrections.is_empty() {
        return text.to_string();
    }

    let correction_map = config.correction_map();
    if correction_map.is_empty() {
        return text.to_string();
    }

    let mut result = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();

    while let Some(&(start, ch)) = chars.peek() {
        if ch.is_alphanumeric() || ch == '\'' || ch == '-' || ch == '·' {
            // Collect a word (including hyphens, apostrophes, middle dots).
            let mut end = start;
            while let Some(&(idx, c)) = chars.peek() {
                if c.is_alphanumeric() || c == '\'' || c == '-' || c == '·' {
                    end = idx + c.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            let word = &text[start..end];
            let lower = word.to_lowercase();

            // Exact match first.
            if let Some(correct) = correction_map.get(&lower) {
                result.push_str(correct);
            } else {
                // Fuzzy match against all patterns. Skip candidates whose
                // length differs by more than 40% - they can't possibly reach
                // the 0.88 Jaro-Winkler threshold and this avoids computing
                // the full similarity for most dictionary entries.
                let mut best_match: Option<(&str, f64)> = None;
                for (pattern, correct) in &correction_map {
                    let len_ratio = lower.len().min(pattern.len()) as f64
                        / lower.len().max(pattern.len()).max(1) as f64;
                    if len_ratio < 0.6 {
                        continue;
                    }
                    let sim = jaro_winkler(&lower, pattern);
                    if sim >= 0.88 && (best_match.is_none() || sim > best_match.unwrap().1) {
                        best_match = Some((correct.as_str(), sim));
                    }
                }
                if let Some((correct, _)) = best_match {
                    result.push_str(correct);
                } else {
                    result.push_str(word);
                }
            }
        } else {
            // Non-word character: copy as-is.
            result.push(ch);
            chars.next();
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CorrectionEntry;

    fn config_with_corrections(entries: Vec<CorrectionEntry>) -> Config {
        Config {
            corrections: entries,
            ..Config::default()
        }
    }

    #[test]
    fn exact_match_correction() {
        let config = config_with_corrections(vec![CorrectionEntry {
            patterns: vec!["kubernetes".to_string(), "kuber netties".to_string()],
            correct: "Kubernetes".to_string(),
        }]);
        assert_eq!(
            apply("I use kubernetes daily", &config),
            "I use Kubernetes daily"
        );
    }

    #[test]
    fn fuzzy_match_correction() {
        let config = config_with_corrections(vec![CorrectionEntry {
            patterns: vec!["kubernetes".to_string()],
            correct: "Kubernetes".to_string(),
        }]);
        // "kuberntes" is a common Whisper misrecognition.
        assert_eq!(
            apply("deploy to kuberntes", &config),
            "deploy to Kubernetes"
        );
    }

    #[test]
    fn no_false_positive_on_dissimilar_words() {
        let config = config_with_corrections(vec![CorrectionEntry {
            patterns: vec!["kubernetes".to_string()],
            correct: "Kubernetes".to_string(),
        }]);
        assert_eq!(apply("I like bananas", &config), "I like bananas");
    }

    #[test]
    fn preserves_punctuation() {
        let config = config_with_corrections(vec![CorrectionEntry {
            patterns: vec!["rust".to_string()],
            correct: "Rust".to_string(),
        }]);
        assert_eq!(apply("hello, rust! world.", &config), "hello, Rust! world.");
    }

    #[test]
    fn empty_corrections_is_identity() {
        let config = config_with_corrections(vec![]);
        assert_eq!(apply("hello world", &config), "hello world");
    }
}
