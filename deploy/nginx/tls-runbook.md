# Runbook：TLS 证书申请 / 续期 / 回滚（certbot + Let's Encrypt）

> 适用：自托管 WordForge 生产环境（Ubuntu 22.04 / 24.04，nginx 1.18+）

本文件配套 `deploy/nginx/sample.conf` 使用，覆盖 v1.1 资源包热更场景下的 TLS 全生命周期 SOP。完整反代细节另见 `docs/runbook/nginx-tls.md`，这里聚焦证书本身的申请、续期、私钥保管与回滚。

---

## 一、首次申请

### 症状（适用场景）

全新服务器，nginx 已就绪并启用 `deploy/nginx/sample.conf`，域名 `api.wordforge.app` 已解析到本机公网 IP，但 `/etc/letsencrypt/live/api.wordforge.app/` 不存在。

### 诊断

```bash
# 域名解析正确
dig +short api.wordforge.app    # 应返回服务器公网 IP

# 80 端口可从公网访问（certbot HTTP-01 校验前置）
curl -sI http://api.wordforge.app/.well-known/acme-challenge/ping
# 期望：HTTP/1.1 404（路径不存在但端口通）

# nginx 临时只挂 HTTP（证书未生成前 HTTPS 段无法 reload）
nginx -t
```

### 处置

```bash
# 1. 安装 certbot 与 nginx 插件
apt update
apt install -y certbot python3-certbot-nginx

# 2. 申请证书（certbot --nginx 会自动改写 nginx 配置填充证书路径）
certbot --nginx \
  -d api.wordforge.app \
  --non-interactive \
  --agree-tos \
  --email ops@wordforge.app    # 续期提醒邮箱，必须真实可达

# 3. 验证证书已生成
certbot certificates
ls -l /etc/letsencrypt/live/api.wordforge.app/
# 期望：fullchain.pem / privkey.pem / cert.pem / chain.pem 全部存在

# 4. nginx reload
nginx -t && systemctl reload nginx

# 5. 端到端验证
curl -sI https://api.wordforge.app/health
curl -sI https://api.wordforge.app | grep -i strict-transport
```

---

## 二、自动续期（cron + systemd timer）

### 症状（适用场景）

- 计划性：上线后必须验证自动续期可用
- 告警：监控提示证书剩余 ≤ 14 天（Let's Encrypt 默认有效期 90 天）

### 诊断

```bash
# 查看到期时间
certbot certificates | grep -E "Domains|Expiry"

# certbot 默认提供 systemd timer，优先使用
systemctl status certbot.timer
# 期望：active (waiting)，每 12 小时触发一次（仅在剩余 < 30 天时实际续期）
```

### 处置

#### 方案 A：systemd timer（首选，certbot 安装自带）

无需额外配置，安装即生效。验证：

```bash
systemctl list-timers certbot.timer
# 期望：NEXT 字段为未来 12 小时内
```

#### 方案 B：cron（systemd 不可用时回退）

```bash
crontab -e
# 添加：每天凌晨 3:17 触发续期（错峰避免高峰整点），续期成功则 reload nginx
17 3 * * * certbot renew --quiet --deploy-hook "systemctl reload nginx"
```

`--deploy-hook` 仅在证书实际被续期时执行 reload，避免无谓重载。

#### 续期 dry-run（强制每次部署后验证）

```bash
certbot renew --dry-run
# 期望：Congratulations, all renewals succeeded
```

任何 nginx 配置变更（特别是 server_name / location 调整）后必须跑一次 dry-run。

---

## 三、私钥保管

### 文件位置与权限

```
/etc/letsencrypt/live/api.wordforge.app/
├── fullchain.pem  → ../../archive/api.wordforge.app/fullchain1.pem
├── privkey.pem    → ../../archive/api.wordforge.app/privkey1.pem  ← 私钥
├── cert.pem       → ../../archive/api.wordforge.app/cert1.pem
└── chain.pem      → ../../archive/api.wordforge.app/chain1.pem
```

certbot 默认权限：

| 路径 | 属主 | 权限 |
|---|---|---|
| `/etc/letsencrypt/live/` | root:root | `drwx------` (0700) |
| `/etc/letsencrypt/archive/` | root:root | `drwx------` (0700) |
| `privkey*.pem` | root:root | `-rw-------` (0600) |

### 验证权限

```bash
stat -c '%U:%G %a %n' /etc/letsencrypt/live /etc/letsencrypt/archive
# 期望：root:root 700 ...

find /etc/letsencrypt/archive -name 'privkey*.pem' -exec stat -c '%U:%G %a %n' {} \;
# 期望：root:root 600 ...
```

### 保管规则

1. **禁止 chmod 放宽**。nginx 以 root 启动 master 进程加载证书，worker 降权后无需读取私钥。任何 `0644` / 添加 group 读权限的尝试都是错误的。
2. **禁止外带**。私钥不出主机。如需备份，整体加密 `/etc/letsencrypt/` 后存离线介质：
   ```bash
   tar czf - /etc/letsencrypt/ | age -r <recipient-pubkey> > letsencrypt-$(date +%F).tar.gz.age
   ```
3. **审计读取**。可选挂 `auditd` 规则记录非 root 读尝试：
   ```bash
   echo '-w /etc/letsencrypt/archive -p r -k letsencrypt_read' >> /etc/audit/rules.d/tls.rules
   augenrules --load
   ```
4. **轮转节奏**。Let's Encrypt 证书 90 天自动轮转，私钥默认随之更新（除非 `--reuse-key`）。怀疑泄露立即走「四、回滚」流程强制换密钥对。

---

## 四、回滚（证书失效 / 私钥泄露快速切换）

### 症状

- 证书突然失效（nginx 启动报 `SSL_CTX_use_PrivateKey_file()` 失败 / 浏览器 `NET::ERR_CERT_*`）
- 证书被 Let's Encrypt 撤销（控制台收到 revocation 通知）
- 私钥泄露需立即换发

### 诊断

```bash
# 1. 当前证书是否有效
openssl x509 -in /etc/letsencrypt/live/api.wordforge.app/fullchain.pem -noout -dates -subject
# 检查 notAfter 是否过期、subject 是否对

# 2. nginx 是否能加载
nginx -t
# 报错示例：
#   nginx: [emerg] cannot load certificate "/etc/letsencrypt/live/.../fullchain.pem"
#   nginx: [emerg] SSL_CTX_use_PrivateKey_file(... privkey.pem) failed

# 3. OCSP 检查（确认是否已被撤销）
openssl ocsp -issuer /etc/letsencrypt/live/api.wordforge.app/chain.pem \
             -cert /etc/letsencrypt/live/api.wordforge.app/cert.pem \
             -text -url $(openssl x509 -in /etc/letsencrypt/live/api.wordforge.app/cert.pem -noout -ocsp_uri)
```

### 处置

#### 路径 A：切换到备用证书（最快，分钟级恢复）

平时维护一份备用证书（同域名、不同 ACME 账户或不同 CA，如 ZeroSSL）：

```bash
/etc/letsencrypt-backup/api.wordforge.app/
├── fullchain.pem
└── privkey.pem
```

切换：

```bash
# 1. 备份当前损坏证书
mv /etc/letsencrypt/live/api.wordforge.app /etc/letsencrypt/live/api.wordforge.app.broken.$(date +%s)

# 2. 软链到备份目录
ln -s /etc/letsencrypt-backup/api.wordforge.app /etc/letsencrypt/live/api.wordforge.app

# 3. reload
nginx -t && systemctl reload nginx

# 4. 验证
curl -sI https://api.wordforge.app/health
openssl s_client -servername api.wordforge.app -connect api.wordforge.app:443 </dev/null 2>/dev/null | openssl x509 -noout -issuer -dates
```

#### 路径 B：强制重新申请（无备份证书时）

```bash
# 1. 撤销旧证书（如确认泄露）
certbot revoke \
  --cert-path /etc/letsencrypt/live/api.wordforge.app/cert.pem \
  --reason keycompromise

# 2. 删除旧密钥对
certbot delete --cert-name api.wordforge.app

# 3. 重新申请（生成全新密钥对）
certbot --nginx \
  -d api.wordforge.app \
  --non-interactive \
  --agree-tos \
  --email ops@wordforge.app

# 4. reload
nginx -t && systemctl reload nginx
```

#### 路径 C：临时降级到 HTTP（最后手段，仅维护窗口）

```bash
# 仅当证书无法快速恢复且必须保业务时使用，恢复后立即关闭
cat > /etc/nginx/sites-available/wordforge-http-only <<'EOF'
server {
    listen 80;
    server_name api.wordforge.app;
    location / { proxy_pass http://127.0.0.1:3000; }
}
EOF
rm /etc/nginx/sites-enabled/wordforge
ln -s /etc/nginx/sites-available/wordforge-http-only /etc/nginx/sites-enabled/
nginx -t && systemctl reload nginx
```

> **警告**：当前生产 iOS 客户端对生产 host 开了 scoped ATS 例外（`NSExceptionAllowsInsecureHTTPLoads`），本身允许 HTTP，此降级不会立即导致 iOS 请求失败——但证书固定（CertPinning）届时同样因明文传输而零保护。此路径仅适用于 admin 临时接入或全面停服公告期；若后续发布已移除该 ATS 例外的客户端版本，则本路径会使那些版本请求失败，需按当时线上客户端版本评估影响。

---

## 五、常见问题

### certbot renew 不实际续期

```bash
certbot renew --dry-run
# 输出 "Cert not yet due for renewal"
```

正常。certbot 仅在剩余 ≤ 30 天时实际续期。`--force-renewal` 可绕过但会触发 Let's Encrypt 速率限制（每域名每周 5 张证书）。

### nginx reload 后旧证书仍生效

旧 worker 进程仍持有旧证书 fd，需 `systemctl reload` 触发 worker rolling restart。`nginx -s reload` 等价。

### 域名变更（增加 SAN）

```bash
certbot --nginx --expand \
  -d api.wordforge.app \
  -d cdn.wordforge.app \
  --non-interactive --agree-tos --email ops@wordforge.app
```

`--expand` 在原证书上增加 SAN，沿用密钥对。

---

## 参考

- `deploy/nginx/sample.conf` — v1.1 反代配置
- `deploy/nginx/wordforge.conf.sample` — v1.0 通用反代配置（含安全头、gzip、SPA）
- `docs/runbook/nginx-tls.md` — 完整 nginx + TLS 初始部署指引
- [certbot 文档](https://certbot.eff.org/instructions?os=ubuntufocal&webserver=nginx)
- [Let's Encrypt 速率限制](https://letsencrypt.org/docs/rate-limits/)
- RFC 8594（Sunset header）
