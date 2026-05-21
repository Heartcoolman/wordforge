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

## 密钥保管规范

- `.env` 文件权限应为 `600`（`chmod 600 /opt/wordforge/.env`）
- minisign 私钥只存在 GitHub Secrets，**不落磁盘**，不入 git
- 密钥轮转后旧 `.env.bak.*` 保留 30 天供回滚参考，之后销毁
- 定期（每季度）审查哪些人有 GitHub 仓库的 Secrets 读取权限
