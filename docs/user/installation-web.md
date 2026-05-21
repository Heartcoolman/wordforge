# Web 客户端安装

## 托管版（由管理员部署）

若管理员已部署 [wordforge-web](https://github.com/Heartcoolman/wordforge-web) 用户学习端，直接访问对应 URL 即可，无需安装任何软件。

1. 浏览器打开管理员提供的地址（如 `https://learn.example.com`）
2. 注册账号或使用已有账号登录
3. 开始学习

支持的浏览器：Chrome / Edge 108+、Firefox 110+、Safari 16+。

## 自托管（运维人员）

### 前置要求

- Node.js ≥ 18
- 后端 WordForge Server 已部署并可访问（[服务器安装指南](/guide/getting-started)）

### 部署步骤

```bash
git clone https://github.com/Heartcoolman/wordforge-web.git
cd wordforge-web
npm install

# 配置环境变量
cp .env.example .env
# 编辑 .env，填写后端地址：
#   VITE_API_BASE_URL=https://your-wordforge-server.example.com

npm run build       # 产物落到 dist/
```

将 `dist/` 目录部署到任意静态文件服务器（Nginx / Caddy / GitHub Pages / Vercel 均可）。

### Nginx 示例配置

```nginx
server {
    listen 443 ssl;
    server_name learn.example.com;

    root /var/www/wordforge-web/dist;
    index index.html;

    # SPA 路由：所有路径回落到 index.html
    location / {
        try_files $uri $uri/ /index.html;
    }

    # 后端 API 反代（可选，避免 CORS）
    location /api/ {
        proxy_pass http://127.0.0.1:3000;
    }
}
```

### 关键环境变量

| 变量 | 说明 |
|---|---|
| `VITE_API_BASE_URL` | 后端服务地址，不含路径；如 `https://api.example.com` |

## 常见问题

**登录后空白页或 404**
- 检查 Nginx `try_files` 是否正确配置（SPA 路由需回落到 index.html）

**请求被 CORS 拒绝**
- 后端 `.env` 中 `CORS_ORIGIN` 需包含 Web 端的来源地址（精确匹配）
- 或使用 Nginx 反代 `/api/` 到后端，统一同源

**手机浏览器访问白屏**
- 确认使用的浏览器版本在支持列表内
- 清除浏览器缓存后重试
