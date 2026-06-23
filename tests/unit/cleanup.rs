mod helpers;

use flow::cleanup::cleanup;
use flow::config::Config;

#[test]
fn test_fast_cleanup_removes_fillers() {
    let config = Config {
        cleanup_backend: "fast".to_string(),
        ..Config::default()
    };
    let result = cleanup(" um hello uh world ", &config).unwrap();
    assert_eq!(result, "hello world");
}

#[test]
fn test_fast_cleanup_no_fillers_unchanged() {
    let config = Config {
        cleanup_backend: "fast".to_string(),
        ..Config::default()
    };
    let text = "this is a clean sentence";
    let result = cleanup(text, &config).unwrap();
    assert_eq!(result, text);
}

#[test]
fn test_fast_cleanup_all_fillers() {
    let config = Config {
        cleanup_backend: "fast".to_string(),
        ..Config::default()
    };
    let result = cleanup(" um uh ", &config).unwrap();
    assert_eq!(result, "");
}

#[test]
fn test_fast_cleanup_case_insensitive() {
    let config = Config {
        cleanup_backend: "fast".to_string(),
        ..Config::default()
    };
    assert_eq!(cleanup("UM hello UH world", &config).unwrap(), "hello world");
}

#[test]
fn test_none_cleanup_returns_input_unchanged() {
    let config = Config {
        cleanup_backend: "none".to_string(),
        ..Config::default()
    };
    let text = "whatever input";
    let result = cleanup(text, &config).unwrap();
    assert_eq!(result, text);
}

#[test]
fn test_command_cleanup_with_valid_echo_command() {
    let config = Config {
        cleanup_backend: "command".to_string(),
        cleanup_command: "cat".to_string(),
        ..Config::default()
    };
    let text = "hello command";
    let result = cleanup(text, &config).unwrap();
    assert_eq!(result, text);
}

#[test]
fn test_command_cleanup_with_empty_command_fails() {
    let config = Config {
        cleanup_backend: "command".to_string(),
        cleanup_command: "".to_string(),
        ..Config::default()
    };
    let result = cleanup("text", &config);
    assert!(result.is_err());
}

#[test]
fn test_command_cleanup_with_failing_command_returns_error() {
    let config = Config {
        cleanup_backend: "command".to_string(),
        cleanup_command: "exit 1".to_string(),
        ..Config::default()
    };
    let result = cleanup("text", &config);
    assert!(result.is_err());
}

#[test]
fn test_ollama_cleanup_using_mock_server() {
    let mut config = Config {
        cleanup_backend: "ollama".to_string(),
        ..Config::default()
    };
    let mock_body = r#"{"response": "cleaned text"}"#.to_string();
    let url = helpers::mock_ollama_server(mock_body);
    config.ollama_url = url;
    config.ollama_model = "test-model".to_string();

    let result = cleanup("raw text", &config).unwrap();
    assert_eq!(result, "cleaned text");
}

#[test]
fn test_dispatcher_routes_to_correct_backend() {
    // none
    let config = Config {
        cleanup_backend: "none".to_string(),
        ..Config::default()
    };
    assert_eq!(cleanup("foo", &config).unwrap(), "foo");

    // fast (explicit)
    let config = Config {
        cleanup_backend: "fast".to_string(),
        ..Config::default()
    };
    assert_eq!(cleanup(" um bar ", &config).unwrap(), "bar");

    // command
    let config = Config {
        cleanup_backend: "command".to_string(),
        cleanup_command: "cat".to_string(),
        ..Config::default()
    };
    assert_eq!(cleanup("baz", &config).unwrap(), "baz");

    // unknown defaults to fast
    let config = Config {
        cleanup_backend: "unknown".to_string(),
        ..Config::default()
    };
    assert_eq!(cleanup(" um qux ", &config).unwrap(), "qux");
}
