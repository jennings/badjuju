use std::process::Command;

const BINARY: &str = env!("CARGO_BIN_EXE_badjuju");

fn init_jj_repo(dir: &std::path::Path) {
    Command::new("jj")
        .args(["git", "init"])
        .current_dir(dir)
        .output()
        .expect("jj git init failed");
}

#[test]
fn cli_status_prints_absolute_path_to_status_jujutsu() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let out = Command::new(BINARY)
        .args(["status"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run badjuju status");

    assert!(
        out.status.success(),
        "expected exit 0, got {}; stderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8(out.stdout).unwrap();
    let path = stdout.trim();
    assert!(
        std::path::Path::new(path).is_absolute(),
        "expected absolute path, got: {path}"
    );
    assert!(
        path.ends_with(".jj/badjuju/status.jujutsu")
            || path.ends_with(".jj\\badjuju\\status.jujutsu"),
        "expected path ending in .jj/badjuju/status.jujutsu, got: {path}"
    );
    assert!(
        std::path::Path::new(path).exists(),
        "status.jujutsu not found at: {path}"
    );
}

#[test]
fn cli_status_outside_workspace_exits_nonzero_with_message() {
    let dir = tempfile::tempdir().unwrap();

    let out = Command::new(BINARY)
        .args(["status"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run badjuju status");

    assert!(
        !out.status.success(),
        "expected non-zero exit, got {}",
        out.status
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.is_empty(),
        "expected a message on stderr but got nothing"
    );
}
