use std::fs;
use std::path::Path;

#[test]
fn run_script_exposes_test_and_run_commands() {
    let script_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("run.sh");
    let contents = fs::read_to_string(&script_path).expect("run.sh should exist");

    assert!(
        contents.starts_with("#!/usr/bin/env bash\n"),
        "unexpected shebang: {contents}"
    );
    assert!(
        contents.contains("case \"${command}\" in"),
        "script should dispatch commands: {contents}"
    );
    assert!(
        contents.contains("\"test\")"),
        "script should expose a test command: {contents}"
    );
    assert!(
        contents.contains("\"run\")"),
        "script should expose a run command: {contents}"
    );
    assert!(
        contents.contains("cargo fmt --check"),
        "script should verify formatting: {contents}"
    );
    assert!(
        contents.contains("cargo test"),
        "script should run tests: {contents}"
    );
    assert!(
        contents.contains("cargo run --"),
        "script should launch the binary through cargo: {contents}"
    );
}
