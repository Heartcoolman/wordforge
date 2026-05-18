use std::process::Command;

fn main() {
    let version = std::env::var("WORDFORGE_BUILD_VERSION")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            Command::new("git")
                .args(["describe", "--tags", "--abbrev=0"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("v{}", env!("CARGO_PKG_VERSION")))
        });

    println!("cargo:rustc-env=GIT_VERSION={version}");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/tags");
    println!("cargo:rerun-if-changed=.git/packed-refs");
}
