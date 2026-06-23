use crate::config::Config;
use anyhow::Result;
use std::io::Write;
use std::process::{Command, Stdio};

pub fn cleanup(text: &str, config: &Config) -> Result<String> {
    if config.cleanup_command.is_empty() {
        anyhow::bail!("cleanup_command is empty");
    }

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&config.cleanup_command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }

    let output = child.wait_with_output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Cleanup command failed: {}", stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
