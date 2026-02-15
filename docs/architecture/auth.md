# 认证机制

系统采用分层认证架构，用户端与管理端完全隔离。

## 认证方式

| 角色 | Access Token | Refresh Token | 存储方式 |
|------|-------------|---------------|---------|
| 用户 | JWT (短期) | JWT (长期) | Access Token 仅内存；Refresh Token 通过 HttpOnly Secure Cookie |
| 管理员 | JWT (独立密钥) | — | sessionStorage |

## 安全策略

- 所有 Token 在服务端以 **SHA-256 哈希**形式存储，原文不落盘
- Refresh Token 采用**一次性消费机制**，防止重放攻击
- **账户锁定**：连续登录失败后自动临时锁定
- 管理员首次使用需通过 `/admin/setup` 初始化

## 用户端认证流程

```
登录 → 获得 accessToken + refreshToken
  → accessToken 存储于内存
  → refreshToken 通过 HttpOnly Secure Cookie 传输
  → 后续请求自动注入 Authorization: Bearer <JWT>
  → 过期前自动刷新 → POST /api/auth/refresh
  → 401 响应 → 清除 token → 跳转登录页
```

## 管理端认证流程

```
首次访问 → GET /api/admin/auth/status 检查初始化状态
  → 未初始化 → 引导创建管理员 → POST /api/admin/auth/setup
  → 已初始化 → 管理员登录 → POST /api/admin/auth/login
  → 使用独立密钥 (ADMIN_JWT_SECRET) 签发 JWT
```
