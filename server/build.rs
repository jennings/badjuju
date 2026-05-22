fn main() {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output();
    let commit = match output {
        Ok(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        }
        _ => "unknown".to_string(),
    };
    println!("cargo:rustc-env=BADJUJU_COMMIT={commit}");
    println!("cargo:rerun-if-changed=../.git/HEAD");
}
