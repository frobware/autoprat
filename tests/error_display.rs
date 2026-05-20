use std::process::Command;

fn autoprat() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_autoprat"));
    // Force a backtrace-capture environment: this is the worst case
    // we want to insulate the user from, since a globally exported
    // RUST_BACKTRACE=1 is a common Rust developer setup.
    cmd.env("RUST_BACKTRACE", "1");
    cmd
}

fn assert_clean_user_error(stderr: &str) {
    assert!(
        stderr.starts_with("Error: "),
        "expected a single-line `Error: ...` message, got: {stderr}"
    );
    assert!(
        !stderr.contains("Stack backtrace"),
        "handled user errors must not include a backtrace, got: {stderr}"
    );
    assert!(
        !stderr.contains("anyhow::"),
        "handled user errors must not leak anyhow internals, got: {stderr}"
    );
}

#[test]
fn missing_required_args_prints_clean_error() {
    let output = autoprat().output().expect("failed to spawn autoprat");

    assert!(
        !output.status.success(),
        "expected nonzero exit when invoked without required args"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Must specify one of"),
        "expected the validation message, got: {stderr}"
    );
    assert_clean_user_error(&stderr);
}

#[test]
fn invalid_pr_identifier_prints_clean_error() {
    let output = autoprat()
        .args(["-r", "openshift/bpfman-operator", "-"])
        .output()
        .expect("failed to spawn autoprat");

    assert!(
        !output.status.success(),
        "expected nonzero exit for invalid PR identifier"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Invalid PR identifier '-'"),
        "expected the parser message, got: {stderr}"
    );
    assert_clean_user_error(&stderr);
}
