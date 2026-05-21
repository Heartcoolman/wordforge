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

    // M0-R2：minisign 公钥编译期嵌入（决策 O5-a）。
    // CI 通过 GH secret MINISIGN_PUBLIC_KEY 注入；本地开发留空时验签被跳过（warn 日志）。
    // 生成密钥对：minisign -G -p wordforge.pub -s wordforge.key
    // 公钥格式：minisign 标准 base64 行（以 "RW" 开头的单行）。
    let pubkey = std::env::var("MINISIGN_PUBLIC_KEY")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_default();
    println!("cargo:rustc-env=MINISIGN_PUBKEY={pubkey}");
    println!("cargo:rerun-if-env-changed=MINISIGN_PUBLIC_KEY");
}
