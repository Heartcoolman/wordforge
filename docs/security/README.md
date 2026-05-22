# WordForge Release 签名公钥

## 公钥

文件：[`wordforge-release.pub`](./wordforge-release.pub)
指纹：`RWQIHmTQvseWZo0Vc1npFBKZ/mMhi1S6eWT8hQ85Cmum5ftRgz87Yqll`
算法：minisign（Ed25519）
生效起始：v1.0.0（2026-05-22）

## 验证 release tarball

```bash
# 1. 安装 minisign（任选其一）
brew install minisign          # macOS
apt install minisign           # Debian/Ubuntu
# 或下载 https://github.com/jedisct1/minisign/releases

# 2. 下载 release tarball + .minisig 签名文件
gh release download v1.0.0 --pattern '*.tar.gz' --pattern '*.minisig'

# 3. 验证签名
minisign -Vm wordforge-linux-x86_64.tar.gz -p docs/security/wordforge-release.pub
# 期望输出：Signature and comment signature verified
```

## 内嵌公钥

WordForge binary 自身也内嵌此公钥（`build.rs` 编译期注入 `MINISIGN_PUBKEY`），admin 一键升级路径通过 `minisign-verify` crate 在客户端验签下载到的新版 tarball，拒绝未签名或公钥不匹配的产物。

## 密钥轮转

见 [`docs/runbook/key-rotation.md`](../runbook/key-rotation.md)。轮转后历史 release 仍可用旧公钥验签；新 release 用新公钥。
