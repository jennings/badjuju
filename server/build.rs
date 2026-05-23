fn main() {
    let version = std::fs::read_to_string("../version")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());
    let version = version.trim().to_string();
    println!("cargo:rustc-env=BADJUJU_VERSION={version}");
    println!("cargo:rerun-if-changed=../version");

    let output = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output();
    let commit = match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => "unknown".to_string(),
    };
    println!("cargo:rustc-env=BADJUJU_COMMIT={commit}");
    println!("cargo:rerun-if-changed=../.git/HEAD");
}
