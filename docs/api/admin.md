# 管理后台 API

管理后台使用独立的 JWT 密钥（`ADMIN_JWT_SECRET`），认证体系与用户端完全隔离。

## 管理员认证

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/api/admin/auth/status` | 检查是否已初始化 Admin |
| POST | `/api/admin/auth/setup` | 首次 Admin 创建（`{ email, password }`） |
| POST | `/api/admin/auth/login` | Admin 登录 |
| POST | `/api/admin/auth/logout` | Admin 登出 |

## 用户管理

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/api/admin/users` | 用户列表 |
| POST | `/api/admin/users/:id/ban` | 封禁用户 |
| POST | `/api/admin/users/:id/unban` | 解封用户 |
| GET | `/api/admin/stats` | 系统统计（`{ users, words, records }`） |

## 数据分析

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/api/admin/analytics/engagement` | 用户参与度（`{ totalUsers, activeToday, retentionRate }`） |
| GET | `/api/admin/analytics/learning` | 学习数据（`{ totalWords, totalRecords, overallAccuracy }`） |

## 系统监控

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/api/admin/monitoring/health` | 系统健康（`{ status, dbSizeBytes, uptime, version }`） |
| GET | `/api/admin/monitoring/database` | 数据库信息（`{ sizeOnDisk, treeCount, trees }`） |

## AMAS 管理（需 Admin）

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/api/amas/config` | 获取 AMAS 配置 |
| PUT | `/api/amas/config` | 更新 AMAS 配置 |
| GET | `/api/amas/metrics` | 算法指标快照 |
| GET | `/api/amas/monitoring` | 监控事件列表（`?limit=50`） |

## 广播与设置

| 方法 | 端点 | 说明 |
|------|------|------|
| POST | `/api/admin/broadcast` | 全局广播（`{ title, message }`） |
| GET | `/api/admin/settings` | 获取系统设置 |
| PUT | `/api/admin/settings` | 更新系统设置 |

### 系统设置模型

```json
{
  "maxUsers": 1000,
  "registrationEnabled": true,
  "maintenanceMode": false,
  "defaultDailyWords": 20
}
```
