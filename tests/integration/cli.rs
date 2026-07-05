use std::process::Command;

fn target_dir() -> String {
    // Ask Cargo where the build artifacts live, since CARGO_TARGET_DIR may be
    // set to an external directory.
    let output = Command::new("cargo")
        .args(["metadata", "--format-version=1", "--no-deps"])
        .output()
        .expect("cargo metadata should run");
    let json = String::from_utf8_lossy(&output.stdout);
    let key = "\"target_directory\":\"";
    let start = json.find(key).expect("target_directory missing") + key.len();
    let end = json[start..].find('"').expect("target_directory unterminated") + start;
    json[start..end].to_string()
}

fn mist_bin() -> Command {
    let target_dir = target_dir();
    let mut cmd = Command::new(format!("{}/debug/mist", target_dir));
    // Point to a non-existent config dir so the test doesn't touch the user's config.
    cmd.env("HOME", "/tmp/mist-cli-test-no-home");
    cmd
}

#[test]
fn cli_help_returns_success() {
    let output = mist_bin().arg("--help").output().expect("failed to run mist --help");
    assert!(output.status.success(), "mist --help failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Local voice dictation daemon"));
}

#[test]
fn cli_model_list_returns_success() {
    let output = mist_bin()
        .arg("model")
        .arg("list")
        .output()
        .expect("failed to run mist model list");
    assert!(output.status.success(), "mist model list failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("small.en"));
}
