use mist::cleanup::cleanup;
use mist::config::Config;
use proptest::prelude::*;

fn random_string() -> impl Strategy<Value = String> {
    proptest::collection::vec(any::<char>(), 0..200).prop_map(|chars| chars.into_iter().collect())
}

fn fast_cleanup(text: &str) -> anyhow::Result<String> {
    let cfg = Config {
        cleanup_backend: "fast".to_string(),
        ..Config::default()
    };
    cleanup(text, &cfg)
}

fn none_cleanup(text: &str) -> anyhow::Result<String> {
    let cfg = Config {
        cleanup_backend: "none".to_string(),
        ..Config::default()
    };
    cleanup(text, &cfg)
}

proptest! {
    #[test]
    fn none_cleanup_is_identity(s in random_string()) {
        let result = none_cleanup(&s).unwrap();
        prop_assert_eq!(result, s);
    }

    #[test]
    fn fast_cleanup_never_panics(s in random_string()) {
        // The important property: never panic on any input.
        let _ = fast_cleanup(&s);
    }

    #[test]
    fn fast_cleanup_is_idempotent(s in random_string()) {
        let once = fast_cleanup(&s).unwrap();
        let twice = fast_cleanup(&once).unwrap();
        prop_assert_eq!(once, twice, "not idempotent for: {}", s);
    }
}
