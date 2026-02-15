# 认证 API

## 用户认证 `/api/auth`

| 方法 | 端点 | 请求体 | 说明 |
|------|------|--------|------|
| POST | `/api/auth/register` | `{ email, username, password }` | 注册 |
| POST | `/api/auth/login` | `{ email, password }` | 登录 |
| POST | `/api/auth/refresh` | — (需 Bearer Token) | 刷新 Token |
| POST | `/api/auth/logout` | — (需 Bearer Token) | 登出 |
| POST | `/api/auth/forgot-password` | `{ email }` | 忘记密码 |
| POST | `/api/auth/reset-password` | `{ token, newPassword }` | 重置密码 |

### 响应示例

注册/登录成功：

```json
{
  "success": true,
  "data": {
    "token": "...",
    "accessToken": "...",
    "refreshToken": "...",
    "user": {
      "id": "...",
      "email": "user@example.com",
      "username": "user1",
      "isBanned": false
    }
  }
}
```

## 用户信息 `/api/users`

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/api/users/me` | 获取当前用户 |
| PUT | `/api/users/me` | 更新用户名 |
| PUT | `/api/users/me/password` | 修改密码（`{ current_password, new_password }`） |
| GET | `/api/users/me/stats` | 用户统计 |

### 用户统计响应

```json
{
  "totalWordsLearned": 150,
  "totalSessions": 30,
  "totalRecords": 1200,
  "streakDays": 7,
  "accuracyRate": 0.85
}
```
