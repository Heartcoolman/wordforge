# Runbook：nginx 反向代理 + TLS（certbot）

> 适用：Ubuntu 22.04 / 24.04，nginx 1.18+，WordForge 自托管生产环境

---

## 概述

WordForge 后端默认监听 `127.0.0.1:3000`，nginx 作为反向代理负责：

- HTTP → HTTPS 强制跳转
- TLS 终止（Let's Encrypt 证书，certbot 自动续期）
- SSE 长连接无缓冲透传（`/api/realtime/events`）
- gzip 压缩 API JSON 和静态资源

配置示例文件：`deploy/nginx/wordforge.conf.sample`

---

## 一、初始部署

### 症状（适用场景）
全新服务器，nginx 尚未配置，域名已解析到服务器 IP。

### 诊断（前置检查）

```bash
# 确认 WordForge 正在运行
systemctl is-active wordforge
curl -s http://127.0.0.1:3000/health | python3 -m json.tool

# 确认域名解析正确（替换 example.com）
dig +short example.com       # 应返回服务器 IP
curl -s http://example.com/health   # 应返回 JSON（HTTP 200）
```

> **注意**：certbot HTTP-01 校验需要 80 端口可从公网访问，且域名必须已解析到本机 IP。

### 处置

#### 1. 安装 nginx 和 certbot

```bash
apt update
apt install -y nginx certbot python3-certbot-nginx
```

#### 2. 拷贝配置文件

```bash
cp /opt/wordforge/deploy/nginx/wordforge.conf.sample \
   /etc/nginx/sites-available/wordforge

# 编辑配置，将 example.com 替换为实际域名
sed -i 's/example\.com/your-domain.com/g' /etc/nginx/sites-available/wordforge

# 启用站点
ln -s /etc/nginx/sites-available/wordforge /etc/nginx/sites-enabled/
rm -f /etc/nginx/sites-enabled/default   # 移除默认站点

# 语法检查
nginx -t
```

#### 3. 临时开放 HTTP（80 端口）供 certbot 校验

此时配置文件中 `listen 443 ssl` 块引用的证书尚不存在，nginx 无法直接 reload。
需先注释掉 HTTPS server 块，仅启用 HTTP 重定向块：

```bash
# 方案：直接用 certbot --nginx 一键完成（推荐）
# certbot 会自动修改 nginx 配置并处理证书路径
systemctl start nginx

certbot --nginx \
  -d your-domain.com \
  --non-interactive \
  --agree-tos \
  --email admin@your-domain.com   # ← 替换为真实邮箱（用于续期提醒）
```

certbot 执行成功后会自动：
- 申请证书并写入 `/etc/letsencrypt/live/your-domain.com/`
- 修改 nginx 配置文件，填充 `ssl_certificate` / `ssl_certificate_key`
- reload nginx

#### 4. 验证

```bash
nginx -t && systemctl reload nginx

# HTTPS 健康检查
curl -s https://your-domain.com/health

# 确认 HTTP → HTTPS 重定向
curl -I http://your-domain.com/health
# 期望：HTTP/1.1 301 Moved Permanently

# 确认 HSTS 头
curl -sI https://your-domain.com | grep -i strict
# 期望：strict-transport-security: max-age=31536000; includeSubDomains

# SSE 连接测试（需要有效 token）
# curl -N -H "Authorization: Bearer <token>" https://your-domain.com/api/realtime/events
```

---

## 二、TLS 证书续期

### 症状（适用场景）
- 计划性：确认自动续期机制正常
- 告警：证书即将在 ≤ 14 天内过期（certbot 默认 30 天内自动续期）

### 诊断

```bash
# 查看证书到期时间
certbot certificates
# 关注 Expiry Date 行

# 确认 certbot 定时任务存在
systemctl list-timers | grep certbot
# 或 cron：
grep -r certbot /etc/cron* /var/spool/cron/ 2>/dev/null
```

### 处置

#### 自动续期（正常路径）

certbot 安装后默认创建 systemd timer：

```bash
systemctl status certbot.timer
# 应为 active (waiting)，每 12 小时运行一次
# 证书剩余不足 30 天时才实际续期
```

如果 timer 不存在，手动创建 cron：

```bash
crontab -e
# 添加：
0 3 * * * certbot renew --quiet && systemctl reload nginx
```

#### 手动强制续期

```bash
# dry-run 验证（不实际续期）
certbot renew --dry-run

# 实际续期
certbot renew --force-renewal
systemctl reload nginx
```

#### 续期后验证

```bash
certbot certificates   # 确认新到期日
nginx -t && systemctl reload nginx
curl -sI https://your-domain.com | grep -i "HTTP/"
```

---

## 三、常见问题

### nginx 启动失败：证书文件不存在

**症状**：`nginx -t` 报 `cannot load certificate "/etc/letsencrypt/live/…"`

**原因**：配置文件中已填写证书路径，但 certbot 尚未运行。

**处置**：

```bash
# 先临时注释 HTTPS server 块，只保留 HTTP 块启动 nginx
# 让 certbot 完成初次申请后再恢复完整配置
systemctl start nginx
certbot --nginx -d your-domain.com ...
```

### SSE 连接频繁断开

**症状**：客户端 EventSource 每 60 秒左右重连一次。

**诊断**：

```bash
grep proxy_read_timeout /etc/nginx/sites-enabled/wordforge
# 应为 75s（SSE 块），与后端 keepalive 15s 相符
```

**处置**：确认 SSE 路由块中 `proxy_read_timeout 75s;` 和 `proxy_buffering off;` 均已设置，重载 nginx。

### 上传词表失败：413 Request Entity Too Large

**症状**：导入大词表时 nginx 返回 413。

**处置**：

```bash
# 查看当前限制
grep client_max_body_size /etc/nginx/sites-enabled/wordforge
# 默认 10m；如需更大，修改 /api/ 块后 reload
```

### 自更新后 nginx 需要重载吗？

**不需要**。WordForge 自更新（`POST /api/admin/updates/apply`）由 systemd 原地替换二进制并重启服务，nginx 反向代理配置无需变更；后端重启期间 nginx 会在 60s 内收到 502，短暂中断后自动恢复。

---

## 参考

- `deploy/nginx/wordforge.conf.sample` — 完整配置示例
- `deploy/wordforge.service.tmpl` — systemd unit 模板
- [certbot 文档](https://certbot.eff.org/instructions?os=ubuntufocal&webserver=nginx)
- [Let's Encrypt 速率限制](https://letsencrypt.org/docs/rate-limits/)（每域名每周 5 张证书）
