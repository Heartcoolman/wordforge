# 快速开始

## 前置要求

- Rust 1.77+（`rustup update stable`）
- Node.js 18+（推荐 20+）

## 1. 克隆与配置

```bash
git clone <repo-url> english
cd english
cp .env.example .env
```

编辑 `.env`，**务必替换以下密钥为强随机值**：

```bash
# 生成密钥
openssl rand -hex 32

# 分别填入
JWT_SECRET=<生成的随机值>
ADMIN_JWT_SECRET=<另一个随机值>
REFRESH_JWT_SECRET=<再一个随机值>
```

## 2. 构建前端

```bash
cd frontend
npm install
npm run build      # 产物输出到 ../static/
cd ..
```

## 3. 启动后端

```bash
cargo run
# 服务默认监听 http://127.0.0.1:3000
```

## 4. 开发模式

同时启动后端和前端开发服务器：

```bash
# 终端 1 — 后端
cargo run

# 终端 2 — 前端（热更新）
cd frontend
npx vite --host    # http://localhost:5173
```

前端开发服务器会将 API 请求代理到后端。
