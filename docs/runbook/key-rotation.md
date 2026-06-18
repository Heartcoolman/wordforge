# Runbook：密钥轮转

> 适用：自托管 WordForge 生产环境

---

## 概述

WordForge 有以下需要定期轮转的密钥：

| 密钥 | 环境变量 | 影响 | 轮转触发条件 |
|---|---|---|---|
| 用户 Access Token 签发密钥 | `JWT_SECRET` | 所有在线用户 Token 立即失效，需重新登录 | 泄露 / 季度轮转 |
| 用户 Refresh Token 签发密钥 | `REFRESH_JWT_SECRET` | Refresh Token 失效，Access 续签中断 | 泄露 / 季度轮转 |
| 管理员 JWT 签发密钥 | `ADMIN_JWT_SECRET` | 管理员 Token 失效，需重新登录 admin | 泄露 / 季度轮转 |
| minisign 签名密钥 | GH Secret `MINISIGN_PRIVATE_KEY` | 旧密钥签发的历史 release 仍可验证（公钥不变）；新私钥签新 release | 私钥泄露 |
| minisign 公钥（编译期嵌入） | GH Secret `MINISIGN_PUBLIC_KEY` + `build.rs` | 需重新发布 binary（二进制内含旧公钥的版本无法验证新私钥签名） | 密钥对整体替换时 |
| 资源包签名密钥（Ed25519） | 私钥文件 `data/keys/ed25519_resource_pack.key` | 存量已签资源包**全部失效需重签**；旧客户端验签失败（公钥客户端硬编码） | 私钥泄露 / 计划性轮换 |

---

## JWT 密钥轮转（JWT_SECRET / REFRESH_JWT_SECRET / ADMIN_JWT_SECRET）

### 症状
- 密钥泄露（日志、环境变量暴露、代码库误提交）
- 季度安全轮转计划

### 诊断

确认当前密钥强度（至少 32 字节随机）：

```bash
# 查看当前密钥长度（不打印值）
grep JWT_SECRET /opt/wordforge/.env | awk -F= '{print length($2), "chars"}'
```

### 处置

**影响评估**：JWT 密钥变更后，**所有在线用户（含管理员）的现有 Token 立即失效**，需重新登录。建议在维护窗口期操作，并提前通知用户。

```bash
INSTALL_DIR=/opt/wordforge

# 1. 生成新密钥（三个必须互不相同）
NEW_JWT=$(openssl rand -hex 32)
NEW_REFRESH=$(openssl rand -hex 32)
NEW_ADMIN=$(openssl rand -hex 32)

# 确认三值不同
echo "$NEW_JWT" "$NEW_REFRESH" "$NEW_ADMIN" | tr ' ' '\n' | sort | uniq -d
# 期望：无输出（无重复）

# 2. 备份当前 .env
cp "$INSTALL_DIR/.env" "$INSTALL_DIR/.env.bak.$(date +%Y%m%d-%H%M%S)"

# 3. 替换密钥（macOS 用 gsed，Linux 用 sed）
sed -i \
  "s|^JWT_SECRET=.*|JWT_SECRET=${NEW_JWT}|" \
  "$INSTALL_DIR/.env"
sed -i \
  "s|^REFRESH_JWT_SECRET=.*|REFRESH_JWT_SECRET=${NEW_REFRESH}|" \
  "$INSTALL_DIR/.env"
sed -i \
  "s|^ADMIN_JWT_SECRET=.*|ADMIN_JWT_SECRET=${NEW_ADMIN}|" \
  "$INSTALL_DIR/.env"

# 4. 验证替换结果（不打印值，只看长度）
grep "JWT_SECRET" "$INSTALL_DIR/.env" | awk -F= '{print $1, length($2), "chars"}'

# 5. 重启服务（新密钥生效）
systemctl restart wordforge

# 6. 验证服务恢复
curl -sf http://127.0.0.1:3000/health
```

### 验证

```bash
# 用旧 Token 请求应返回 401（Token 已失效）
curl -sf -H "Authorization: Bearer <old-token>" \
  http://127.0.0.1:3000/api/users/me \
  | jq '.success'
# 期望：false 或 401

# 重新登录获取新 Token
curl -sf -X POST http://127.0.0.1:3000/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"user@example.com","password":"..."}' \
  | jq '.data.token'
```

---

## minisign 私钥泄露应急

### 症状
- `MINISIGN_PRIVATE_KEY`（GitHub Secret）疑似泄露（CI 日志误打印、仓库误提交、密钥共享给不可信人员）
- 收到安全报告：有人使用伪造签名的 tarball 通过了验签

### 诊断

确认当前二进制内嵌的公钥（需在服务器上运行）：

```bash
# 查看编译时嵌入的公钥（如果服务支持 /api/admin/updates/status 可从版本信息判断）
strings /opt/wordforge/wordforge | grep "^RW"
# RWxxx... 开头的行即为 minisign 公钥
```

### 处置

私钥泄露需要**重新生成密钥对**，并**重新发布所有支持的二进制版本**。步骤如下：

```bash
# 步骤 1：生成新密钥对（本地安全机器上执行）
minisign -G -p wordforge-new.pub -s wordforge-new.key
# 私钥：wordforge-new.key（妥善保管，不上传）
# 公钥：wordforge-new.pub（内容为 "RW..." 开头的一行）

cat wordforge-new.pub
```

**步骤 2：更新 GitHub Repository Secrets**

1. 打开 `https://github.com/Heartcoolman/wordforge/settings/secrets/actions`
2. 更新 `MINISIGN_PRIVATE_KEY`：粘贴 `wordforge-new.key` 的完整内容（含 `untrusted comment:` 行）
3. 更新 `MINISIGN_PUBLIC_KEY`：粘贴 `wordforge-new.pub` 的 `RW...` 那一行

**步骤 3：重新发布 release**（公钥编译期嵌入，需重新构建二进制）

```bash
# 对当前最新 tag 重新触发 release workflow
# 方式：在 GitHub Actions 页面手动 re-run，或推新 tag
git tag -d v0.x.y-new-pub
git tag v0.x.y-new-pub
git push origin v0.x.y-new-pub
```

> **重要**：旧二进制内嵌旧公钥，无法验证新私钥签名的 tarball——升级时 updater 会拒绝新 release。用户必须手动下载并替换到新构建的二进制后，才能继续使用自动更新。

**步骤 4：公告**

在 GitHub Security Advisory 或 release notes 中声明：
- 旧私钥已作废（给出日期）
- 所有使用旧二进制（< vX.Y.Z-new-pub）的实例请手动升级

**步骤 5：清理本地密钥文件**

```bash
# 轮转完成后销毁本地私钥文件
shred -u wordforge-new.key  # Linux
# 或：rm -P wordforge-new.key  # macOS
```

---

## minisign 公钥轮转（计划性密钥对替换）

与私钥泄露应急流程相同，区别在于：
- 可以提前公告，给用户留出升级窗口
- 不需要紧急重发 release，可随下一次正常 release 一并切换

建议步骤：
1. 生成新密钥对
2. 更新 GH Secrets
3. 下一个 stable release 时生效（新 release 用新私钥签，binary 内含新公钥）
4. 同步更新文档，记录旧公钥失效日期

---

## 资源包签名密钥（Ed25519）轮换

> 资源包热更的签名链路与 minisign（二进制自更新）刻意解耦：minisign 签 binary，Ed25519 签 `payload.json`。机制见 [资源包热更](/resource-packs)，发布/排障见 [资源包运维](/runbook/resource-pack-ops)。

### 症状
- 私钥 `data/keys/ed25519_resource_pack.key` 疑似泄露（误提交、备份外泄、共享给不可信人员）
- keys 目录丢失后服务重新生成了新密钥对（公钥与客户端硬编码值不符）
- 计划性密钥对替换

### 诊断

```bash
INSTALL_DIR=/opt/wordforge

# 1. 私钥位置与权限（须 0600）
ls -l "$INSTALL_DIR/data/keys/ed25519_resource_pack.key"

# 2. 当前服务端公钥（应等于客户端硬编码生产公钥）
curl -sf http://127.0.0.1:3000/api/resource-packs/public-key | jq -r '.publicKey'

# 3. 客户端硬编码的生产公钥（三端唯一信任锚）
echo "fr+eALsS/N3gz4AZmpSm/wDbtDCh596WjapwVPtHn6s="
# 二者不一致 = 现网客户端验签会失败
```

> keys 目录是 DB 文件（`DATABASE_URL`）所在目录下的 `keys/` 子目录，默认 `data/learning.db` → `data/keys/`。私钥 32 字节原始字节（权限 0600），公钥 `.pub`（0644），首次启动若不存在则自动生成。

### 处置

资源包签名密钥的**公钥被三端客户端硬编码**作为唯一验签信任锚（`GET /api/resource-packs/public-key` 仅供运行时自检对比，不一致只告警，仍以硬编码为准）。因此轮换有两个不可省略的约束：

1. **不能只换服务端**：仅替换 `data/keys/` 私钥而不发版，旧客户端仍用硬编码的旧公钥验签，新私钥签出的所有包**验签全败**（资源包「变砖」）。
2. **存量已签包全部失效**：所有曾用旧私钥签名落盘的 `static/packs/<pack>/<ver>/payload.json` 的签名对应旧公钥，换密钥后必须**逐个重签重发**（上传新版本号）。

**步骤 1：生成新密钥对**（受控机器上执行，或交由后端首次启动自动生成）

```bash
INSTALL_DIR=/opt/wordforge
KEY_DIR="$INSTALL_DIR/data/keys"

# 备份旧密钥对（保留供回滚/审计）
cp "$KEY_DIR/ed25519_resource_pack.key" "$KEY_DIR/ed25519_resource_pack.key.bak.$(date +%Y%m%d-%H%M%S)"
cp "$KEY_DIR/ed25519_resource_pack.pub" "$KEY_DIR/ed25519_resource_pack.pub.bak.$(date +%Y%m%d-%H%M%S)"

# 删除旧密钥对，让后端下次启动自动生成新对（生成即 0600/0644）
rm "$KEY_DIR/ed25519_resource_pack.key" "$KEY_DIR/ed25519_resource_pack.pub"
systemctl restart wordforge

# 读出新公钥（base64，44 字符）
curl -sf http://127.0.0.1:3000/api/resource-packs/public-key | jq -r '.publicKey'
```

**步骤 2：客户端硬编码新公钥并发版（三端）**

把步骤 1 的新公钥替换三端硬编码的旧值 `fr+eALsS/N3gz4AZmpSm/wDbtDCh596WjapwVPtHn6s=`，并**发布 iOS / Android / web 新版本**。

> **关键时序**：必须让客户端**先发版铺开新公钥**，再用新私钥对生产通道激活新包；否则未升级到新版的存量客户端会对新签包验签失败。计划性轮换可借一次正常的三端发版窗口切换，把公钥变更随版本一同下发。

**步骤 3：重签所有存量资源包**

旧签名对应旧公钥，换密钥后已激活的包对新客户端验签会失败。对每个仍在用的 pack：用**新私钥**（即换密钥后的生产后端）重新上传一个新版本号的 payload，并对相应通道切激活（旧版本号因去重不可覆盖，须用新版本号）。重签后按 [资源包运维 §首包冒烟](/runbook/resource-pack-ops) 用真机确认新客户端验签通过。

**步骤 4：公告与清理**

- 在 release notes 声明旧公钥失效日期；未升级到含新公钥版本的客户端将无法消费新资源包（旧客户端保持本地缓存现状，不会崩溃，但收不到新内容）。
- 轮换确认无误后销毁旧私钥备份。

```bash
shred -u /opt/wordforge/data/keys/ed25519_resource_pack.key.bak.*  # Linux
# 或：rm -P ...  # macOS
```

---

## 密钥保管规范

- `.env` 文件权限应为 `600`（`chmod 600 /opt/wordforge/.env`）
- minisign 私钥只存在 GitHub Secrets，**不落磁盘**，不入 git
- 资源包 Ed25519 私钥 `data/keys/ed25519_resource_pack.key` 权限须 `600`，**不入 git**（确保 `.gitignore` 覆盖 `data/keys/`），并随 DB 一同备份——丢失等同被迫轮换（须客户端重发版）
- 密钥轮转后旧 `.env.bak.*` / 旧密钥备份保留 30 天供回滚参考，之后销毁
- 定期（每季度）审查哪些人有 GitHub 仓库的 Secrets 读取权限
