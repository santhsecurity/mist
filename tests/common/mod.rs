#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread;

/// Spawn a tiny HTTP server on a random port that serves a single response.
/// Returns the base URL (e.g. "<http://127.0.0.1:12345>").
pub fn mock_ollama_server(response_body: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

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

        let body = response_body;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    });

    std::thread::sleep(std::time::Duration::from_millis(50));
    format!("http://127.0.0.1:{port}")
}

/// Generate a short synthetic audio buffer (silence + a sine wave burst).
pub fn synthetic_audio(sample_rate: u32, duration_secs: f32) -> Vec<f32> {
    let total_samples = (sample_rate as f32 * duration_secs) as usize;
    let mut samples = vec![0.0f32; total_samples];
    let start = total_samples / 3;
    let end = start * 2;
    for (i, sample) in samples.iter_mut().enumerate().skip(start).take(end - start) {
        let t = i as f32 / sample_rate as f32;
        *sample = (t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5;
    }
    samples
}

/// Generate a valid TOML config string with custom fields.
pub fn config_toml(hotkey: &str, model: &str, backend: &str) -> String {
    format!(
        r#"hotkey = "{hotkey}"
model = "{model}"
language = "en"
cleanup_backend = "{backend}"
cleanup_enabled = true
live_stream = false
ollama_model = "qwen3:0.6b"
ollama_url = "http://localhost:11434"
cleanup_prompt = "Clean up this text."
cleanup_command = ""
dictionary = ["Rust", "LLM"]
"#
    )
}
