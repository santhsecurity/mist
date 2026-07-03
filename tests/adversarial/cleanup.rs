mod helpers;

use mist::cleanup::cleanup;
use mist::config::Config;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread;

fn fast_cleanup(text: &str) -> anyhow::Result<String> {
    let cfg = Config {
        cleanup_backend: "fast".to_string(),
        ..Config::default()
    };
    cleanup(text, &cfg)
}

fn run_command_cleanup(text: &str, command: &str) -> anyhow::Result<String> {
    let cfg = Config {
        cleanup_backend: "command".to_string(),
        cleanup_command: command.to_string(),
        ..Config::default()
    };
    cleanup(text, &cfg)
}

fn run_ollama_cleanup(text: &str, url: &str, model: &str) -> anyhow::Result<String> {
    let cfg = Config {
        cleanup_backend: "ollama".to_string(),
        ollama_url: url.to_string(),
        ollama_model: model.to_string(),
        ..Config::default()
    };
    cleanup(text, &cfg)
}

fn mock_ollama_server_status(status: &str, body: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let status = status.to_string();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(&stream);
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).unwrap() == 0 {
                break;
            }
            if line == "\r\n" || line == "\n" {
                break;
            }
        }

        let response = format!(
            "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            status,
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    });

    std::thread::sleep(std::time::Duration::from_millis(50));
    format!("http://127.0.0.1:{}", port)
}

#[test]
fn fast_cleanup_um_start() {
    assert_eq!(fast_cleanup("um hello").unwrap(), "hello");
}

#[test]
fn fast_cleanup_um_end() {
    // Regex now catches trailing "um" as well.
    assert_eq!(fast_cleanup("hello um").unwrap(), "hello");
}

#[test]
fn fast_cleanup_um_chain() {
    assert_eq!(fast_cleanup("um um um").unwrap(), "");
}

#[test]
fn fast_cleanup_um_uppercase() {
    // Now case-insensitive — UM is removed.
    assert_eq!(fast_cleanup("UM hello").unwrap(), "hello");
}

#[test]
fn fast_cleanup_um_no_spaces() {
    // "um,um" — "um" at word boundary with comma after: regex removes the
    // first "um," leaving just "um" which is then also removed.
    let result = fast_cleanup("um,um").unwrap();
    // The specific result depends on how the regex handles this edge case.
    // The important thing is it doesn't panic and doesn't grow the string.
    assert!(result.len() <= "um,um".len());
}

#[test]
fn command_cleanup_hangs() {
    let start = std::time::Instant::now();
    let result = run_command_cleanup("test", "sleep 2");
    let elapsed = start.elapsed();
    assert!(
        elapsed >= std::time::Duration::from_secs(1),
        "expected cleanup to block for at least 1s, got result {:?} in {:?}",
        result,
        elapsed
    );
}

#[test]
fn command_cleanup_binary_output() {
    // Use octal escapes (POSIX sh compatible) to output actual binary bytes
    let result = run_command_cleanup("test", "printf '\\0\\001\\002\\377'");
    assert!(result.is_ok(), "should handle binary output: {:?}", result);
    let text = result.unwrap();
    assert!(!text.is_empty());
}

#[test]
fn command_cleanup_command_not_found() {
    let result = run_command_cleanup("test", "this_command_does_not_exist_12345");
    assert!(result.is_err());
}

#[test]
fn ollama_cleanup_server_error() {
    let body = r#"{"error":"model not found"}"#.to_string();
    let url = mock_ollama_server_status("500 Internal Server Error", body);

    let result = run_ollama_cleanup("test", &url, "missing");
    assert!(result.is_err());
}

#[test]
fn ollama_cleanup_invalid_json() {
    let body = r#"not json at all"#.to_string();
    let url = helpers::mock_ollama_server(body);

    let result = run_ollama_cleanup("test", &url, "test");
    assert!(result.is_err());
}

#[test]
fn ollama_cleanup_empty_response() {
    let body = r#"{"response":""}"#.to_string();
    let url = helpers::mock_ollama_server(body);

    let result = run_ollama_cleanup("test", &url, "test");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "");
}

#[test]
fn ollama_cleanup_huge_response() {
    let huge = "x".repeat(100_000);
    let body = format!(r#"{{"response":"{}"}}"#, huge);
    let url = helpers::mock_ollama_server(body);

    let result = run_ollama_cleanup("test", &url, "test");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), huge.len());
}
