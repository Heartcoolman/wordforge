# API 总览

WordForge 后端提供 RESTful API，所有接口统一响应格式：

```typescript
// 成功
{ success: true, data: T }

// 失败
{ success: false, error: string, code: string, message: string, traceId?: string }
```

## 认证方式

请求头携带 JWT：

```
Authorization: Bearer <JWT>
```

或通过 Cookie：`token=<JWT>`

## API 模块

| 模块 | 基路径 | 说明 |
|------|--------|------|
| [认证](/api/auth) | `/api/auth` | 注册、登录、刷新、登出、密码重置 |
| [学习](/api/learning) | `/api/learning`, `/api/records`, `/api/study-config`, `/api/amas` | 学习会话、答题记录、学习配置、AMAS 算法 |
| [单词管理](/api/words) | `/api/words`, `/api/wordbooks`, `/api/word-states` | 单词 CRUD、词书管理、学习状态 |
| [管理后台](/api/admin) | `/api/admin` | 用户管理、系统监控、数据分析、系统设置 |
| 用户 | `/api/users` | 用户信息、统计 |
| 用户画像 | `/api/user-profile` | 奖励偏好、认知画像、学习风格、时间类型 |
| 通知 | `/api/notifications` | 通知列表、已读标记、徽章、偏好 |
| 内容增强 | `/api/content` | 词源分析、语义搜索、词素拆解、混淆词对 |
| 实时事件 | `/api/realtime/events` | SSE 连接，推送 AMAS 状态变更 |
| 健康检查 | `/health` | 存活探测、就绪探测、数据库健康、算法指标 |
