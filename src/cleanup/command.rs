use crate::config::Config;
use anyhow::Result;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Maximum time to wait for the cleanup command before killing it.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

/// Run a user-configured shell command for text cleanup.
///
/// # Security
///
/// `config.cleanup_command` is passed directly to `sh -c` and executed as a
/// shell command. The transcribed text is delivered via stdin (not interpolated
/// into the command string), so it is not subject to shell injection. However,
/// the command value itself **is** trusted user configuration — do not populate
/// it from untrusted sources.
///
/// # Timeout
///
/// The command is killed after 30 seconds to prevent hung processes from
/// blocking the dictation pipeline.
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
        // Drop stdin to signal EOF so the child can finish.
    }

    // Wait with timeout — spawn a thread to wait, and kill if it takes too long.
    let start = std::time::Instant::now();
    loop {
        match child.try_wait()? {
            Some(status) => {
                let output = child.wait_with_output()?;
                if !status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!(
                        "Cleanup command failed (exit {}): {}",
                        status.code().unwrap_or(-1),
                        stderr.trim()
                    );
                }
                return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
            }
            None => {
                if start.elapsed() > COMMAND_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    anyhow::bail!(
                        "Cleanup command timed out after {}s and was killed. \
                         Command: {}",
                        COMMAND_TIMEOUT.as_secs(),
                        config.cleanup_command
                    );
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}
