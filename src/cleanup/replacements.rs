//! Post-cleanup phrase replacement layer.
//!
//! Applies deterministic whole-phrase replacements after cleanup and
//! vocabulary corrections. Useful for expanding shortcuts ("my email" →
//! "user@example.com") or normalizing recurring phrases.

use crate::config::ReplacementEntry;
use regex::Regex;

/// Apply all replacement entries to the text.
///
/// Each pattern is matched case-insensitively as a whole phrase. Replacements
/// are applied in order; earlier entries take precedence over later ones.
pub fn apply(text: &str, replacements: &[ReplacementEntry]) -> String {
    if replacements.is_empty() {
        return text.to_string();
    }

    let mut result = text.to_string();
    for entry in replacements {
        let escaped = regex::escape(&entry.pattern);
        let pattern = format!(r"(?i)\b{escaped}\b");
        if let Ok(re) = Regex::new(&pattern) {
            result = re
                .replace_all(&result, entry.replacement.as_str())
                .to_string();
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(pattern: &str, replacement: &str) -> ReplacementEntry {
        ReplacementEntry {
            pattern: pattern.to_string(),
            replacement: replacement.to_string(),
        }
    }

    #[test]
    fn simple_replacement() {
        let reps = vec![entry("my email", "hi@example.com")];
        assert_eq!(
            apply("Send it to my email please", &reps),
            "Send it to hi@example.com please"
        );
    }

    #[test]
    fn case_insensitive() {
        let reps = vec![entry("kubernetes", "Kubernetes")];
        assert_eq!(apply("deploy to KUBERNETES", &reps), "deploy to Kubernetes");
    }

    #[test]
    fn empty_replacements_is_identity() {
        assert_eq!(apply("hello world", &[]), "hello world");
    }

    #[test]
    fn respects_word_boundaries() {
        let reps = vec![entry("cat", "feline")];
        assert_eq!(apply("concatenate", &reps), "concatenate");
    }
}
