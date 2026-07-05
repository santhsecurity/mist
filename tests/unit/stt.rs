use mist::stt;

#[test]
fn test_list_models_includes_common_choices() {
    let names: Vec<&str> = stt::list_models().iter().map(|m| m.name).collect();
    assert!(names.contains(&"tiny.en"));
    assert!(names.contains(&"small.en"));
    assert!(names.contains(&"base.en"));
}

#[test]
fn test_model_path_uses_expected_filename() {
    let path = stt::model_path("small.en").expect("model_path should succeed");
    let file = path.file_name().unwrap().to_str().unwrap();
    assert_eq!(file, "ggml-small.en.bin");
}

#[test]
fn test_unknown_model_path_still_formats() {
    // The function itself is not the gatekeeper for supported names; it just
    // builds the canonical filename.
    let path = stt::model_path("custom-model").expect("model_path should succeed");
    assert!(path.to_string_lossy().contains("ggml-custom-model.bin"));
}
