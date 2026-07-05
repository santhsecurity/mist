use anyhow::Result;
use regex::Regex;
use std::sync::OnceLock;

/// Compiled regex patterns for filler removal. Built once, reused forever.
struct FillerPatterns {
    fillers: Regex,
    leading: Regex,
    trailing: Regex,
    multi_space: Regex,
}

static PATTERNS: OnceLock<FillerPatterns> = OnceLock::new();

fn patterns() -> &'static FillerPatterns {
    PATTERNS.get_or_init(|| FillerPatterns {
        // Case-insensitive filler words with word boundaries.
        // Only removes filler words that are used as verbal tics, not
        // legitimate uses (e.g. "you know" as a filler vs "do you know").
        fillers: Regex::new(r"(?i)\b(um|uh|er|uhm|umm)\b[,.]?\s*").unwrap(),
        leading: Regex::new(r"(?i)^(um|uh|er|uhm|umm)[,.]?\s*").unwrap(),
        trailing: Regex::new(r"(?i)\s*(um|uh|er|uhm|umm)[,.]?\s*$").unwrap(),
        multi_space: Regex::new(r"  +").unwrap(),
    })
}

pub fn cleanup(text: &str) -> Result<String> {
    let p = patterns();
    let mut text = text.to_string();

    // Remove leading filler.
    text = p.leading.replace(&text, "").to_string();
    // Remove trailing filler.
    text = p.trailing.replace(&text, "").to_string();
    // Remove interior fillers.
    text = p.fillers.replace_all(&text, " ").to_string();
    // Collapse multiple spaces.
    text = p.multi_space.replace_all(&text, " ").to_string();

    Ok(text.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_fillers_case_insensitive() {
        assert_eq!(cleanup("Um hello Uh world").unwrap(), "hello world");
        assert_eq!(cleanup("UM hello UH world").unwrap(), "hello world");
    }

    #[test]
    fn preserves_like_as_verb() {
        // "like" is NOT removed - it has legitimate uses as a verb/preposition.
        let text = "I like dogs";
        assert_eq!(cleanup(text).unwrap(), "I like dogs");
    }

    #[test]
    fn handles_fillers_with_punctuation() {
        assert_eq!(cleanup("um, hello uh. world").unwrap(), "hello world");
    }

    #[test]
    fn idempotent() {
        let input = " um uh hello um world uh ";
        let once = cleanup(input).unwrap();
        let twice = cleanup(&once).unwrap();
        assert_eq!(once, twice);
    }
}
