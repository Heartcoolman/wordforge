# 前端开发计划 — SolidJS + TypeScript

## 一、项目概述

基于现有 Rust (Axum + Sled) 后端，使用 **SolidJS + TypeScript + Vite + Tailwind CSS** 构建自适应英语词汇学习前端。

**原则：一切以后端实际 API 为准，不做后端不支持的功能。**

---

## 二、技术栈

| 领域 | 选型 | 说明 |
|---|---|---|
| 框架 | SolidJS 1.9+ | 细粒度响应式，无虚拟 DOM |
| 语言 | TypeScript 5.x | 类型安全 |
| 构建 | Vite 6 | 极速 HMR |
| 路由 | @solidjs/router | 官方路由 |
| 样式 | Tailwind CSS 4 | 原子化 CSS |
| 状态管理 | SolidJS 内置 Signal + Store | 无需额外状态库 |
| 服务器状态 | @tanstack/solid-query | 数据缓存/同步 |
| 动画 | solid-motionone 或 CSS transitions | 轻量动画 |
| 图标 | Phosphor Icons (solid 版) 或 Lucide | |
| 数据验证 | Zod | 运行时校验 |
| 多入口 | Vite 多入口 (index.html + admin.html) | 用户端 + 管理后台 |

---

## 三、后端实际 API 清单

### 3.1 认证 `/api/auth`

| 方法 | 端点 | 请求体 | 响应 | 说明 |
|---|---|---|---|---|
| POST | `/api/auth/register` | `{ email, username, password }` | `{ token, accessToken, refreshToken, user: { id, email, username, isBanned } }` | 注册 |
| POST | `/api/auth/login` | `{ email, password }` | 同上 | 登录 |
| POST | `/api/auth/refresh` | — (需 Bearer Token) | 同上 | 刷新 Token |
| POST | `/api/auth/logout` | — (需 Bearer Token) | `{ loggedOut: true }` | 登出 |
| POST | `/api/auth/forgot-password` | `{ email }` | `{ success: true }` | 忘记密码（未对接邮件） |
| POST | `/api/auth/reset-password` | `{ token, newPassword }` | `{ success: true }` | 重置密码 |

**认证方式**：`Authorization: Bearer <JWT>` 或 Cookie `token=<JWT>`

### 3.2 用户 `/api/users`

| 方法 | 端点 | 请求体 | 响应 | 说明 |
|---|---|---|---|---|
| GET | `/api/users/me` | — | `UserProfile` | 获取当前用户 |
| PUT | `/api/users/me` | `{ username? }` | `UserProfile` | 更新用户名 |
| PUT | `/api/users/me/password` | `{ current_password, new_password }` | `{ passwordChanged: true }` | 修改密码（snake_case） |
| GET | `/api/users/me/stats` | — | `{ totalWordsLearned, totalSessions, totalRecords, streakDays, accuracyRate }` | 用户统计 |

### 3.3 单词 `/api/words`

| 方法 | 端点 | 请求体/参数 | 响应 | 说明 |
|---|---|---|---|---|
| GET | `/api/words` | `?limit=20&offset=0&search=xxx` | `{ items: Word[], total, limit, offset }` | 单词列表（支持搜索） |
| GET | `/api/words/:id` | — | `Word` | 单词详情 |
| POST | `/api/words` | `{ text, meaning, pronunciation?, partOfSpeech?, difficulty?, examples?, tags?, id? }` | `Word` | 创建单词 |
| PUT | `/api/words/:id` | 同上 | `Word` | 更新单词 |
| DELETE | `/api/words/:id` | — | `{ deleted: true, id }` | 删除单词 |
| POST | `/api/words/batch` | `{ words: [...] }` | `{ count, skipped, items }` | 批量创建 |
| GET | `/api/words/count` | — | `{ total }` | 单词总数 |
| POST | `/api/words/import-url` | `{ url }` | `{ imported, items }` | URL 导入（后端代理抓取） |

**Word 模型**：
```typescript
interface Word {
  id: string;
  text: string;          // 单词拼写
  meaning: string;       // 释义（单个字符串）
  pronunciation?: string;
  partOfSpeech?: string;
  difficulty: number;    // 0-1
  examples: string[];
  tags: string[];
  embedding?: number[];
  createdAt: string;     // ISO 8601
}
```

### 3.4 学习记录 `/api/records`

| 方法 | 端点 | 请求体/参数 | 响应 | 说明 |
|---|---|---|---|---|
| GET | `/api/records` | `?limit=50&offset=0` | `LearningRecord[]` | 获取学习记录（分页） |
| POST | `/api/records` | 见下 | `{ record, amasResult }` | 提交答题记录 + AMAS 处理 + 自动更新 word_learning_states + 自动更新 learning_session 计数 |
| POST | `/api/records/batch` | `{ records: [CreateRecordRequest] }` | `{ count, items: [{ record, amasResult }] }` | 批量提交 |
| GET | `/api/records/statistics` | — | `{ total, correct, accuracy }` | 基础统计 |
| GET | `/api/records/statistics/enhanced` | — | `{ total, correct, accuracy, streak, daily: [{ date, total, correct, accuracy }] }` | 增强统计（含每日分组 + 连续天数） |

**CreateRecord 请求体**：
```typescript
interface CreateRecordRequest {
  wordId: string;
  isCorrect: boolean;
  responseTimeMs: number;
  sessionId?: string;
  isQuit?: boolean;
  dwellTimeMs?: number;
  pauseCount?: number;
  switchCount?: number;
  retryCount?: number;
  focusLossDurationMs?: number;
  interactionDensity?: number;
  pausedTimeMs?: number;
  hintUsed?: boolean;
}
```

**重要**：POST `/api/records` 是学习流程的核心端点。除了创建记录，后端还会：
1. 调用 AMAS 引擎处理事件，返回 `amasResult`
2. 根据 `amasResult.wordMastery` 自动更新 `word_learning_states`（掌握度/状态迁移/复习间隔）
3. 如果带了 `sessionId`，自动更新 `learning_session` 的 `totalQuestions` 和 `actualMasteryCount`

### 3.5 学习会话 `/api/learning`

| 方法 | 端点 | 请求体 | 响应 | 说明 |
|---|---|---|---|---|
| POST | `/api/learning/session` | — (需认证) | `{ sessionId, status, resumed }` | 创建/恢复学习会话（自动关闭其他活跃会话） |
| POST | `/api/learning/study-words` | — (需认证) | `{ words: Word[], strategy: { difficultyRange, newRatio, batchSize } }` | 获取学习单词（基于 AMAS 策略 + 词书配置 + word_learning_states） |
| POST | `/api/learning/next-words` | `{ excludeWordIds, masteredWordIds? }` | `{ words: Word[], batchSize }` | 获取下一批单词（排除已有，标记已掌握） |
| POST | `/api/learning/adjust-words` | `{ userState?, recentPerformance? }` | `{ adjustedStrategy }` | 根据用户状态动态调整策略 |
| POST | `/api/learning/sync-progress` | `{ sessionId, totalQuestions?, contextShifts? }` | `LearningSession` | 同步会话进度（只增不减） |

**学习会话是后端管理的实体**：
- 后端维护 `LearningSession` 实体（id/userId/status/targetMasteryCount/totalQuestions/actualMasteryCount/contextShifts）
- `POST /api/learning/session` 创建新会话或恢复已有活跃会话
- `POST /api/records` 自动递增 session 的 totalQuestions 和 actualMasteryCount
- `POST /api/learning/sync-progress` 同步前端额外进度数据

### 3.6 学习配置 `/api/study-config`

| 方法 | 端点 | 请求体 | 响应 | 说明 |
|---|---|---|---|---|
| GET | `/api/study-config` | — (需认证) | `StudyConfig` | 获取学习配置 |
| PUT | `/api/study-config` | `{ selectedWordbookIds?, dailyWordCount?, studyMode?, dailyMasteryTarget? }` | `StudyConfig` | 更新配置 |
| GET | `/api/study-config/today-words` | — (需认证) | `{ words: Word[], target }` | 今日学习单词 |
| GET | `/api/study-config/progress` | — (需认证) | `{ studied, target, new, learning, reviewing, mastered }` | 学习进度 |

**StudyConfig 模型**：
```typescript
interface StudyConfig {
  userId: string;
  selectedWordbookIds: string[];
  dailyWordCount: number;       // 1-200，默认 20
  studyMode: "MASTERY" | "REVIEW" | "MIXED";
  dailyMasteryTarget: number;   // 1-100
}
```

### 3.7 词书 `/api/wordbooks`

| 方法 | 端点 | 请求体/参数 | 响应 | 说明 |
|---|---|---|---|---|
| GET | `/api/wordbooks/system` | — | `Wordbook[]` | 系统词书列表 |
| GET | `/api/wordbooks/user` | — (需认证) | `Wordbook[]` | 用户词书列表 |
| POST | `/api/wordbooks` | `{ name, description? }` (需认证) | `Wordbook` | 创建用户词书 |
| GET | `/api/wordbooks/:id/words` | `?limit=20&offset=0` | `{ items: Word[], total, limit, offset }` | 词书内单词（分页） |
| POST | `/api/wordbooks/:id/words` | `{ wordIds: string[] }` | `{ added }` | 向词书添加单词 |
| DELETE | `/api/wordbooks/:id/words/:word_id` | — | `{ removed: true }` | 从词书移除单词 |

**Wordbook 模型**：
```typescript
interface Wordbook {
  id: string;
  name: string;
  description: string;
  bookType: "System" | "User";
  userId?: string;
  wordCount: number;
  createdAt: string;
}
```

### 3.8 单词学习状态 `/api/word-states`

| 方法 | 端点 | 请求体/参数 | 响应 | 说明 |
|---|---|---|---|---|
| GET | `/api/word-states/:word_id` | — (需认证) | `WordLearningState` | 查询单词学习状态 |
| POST | `/api/word-states/batch` | `{ wordIds }` | `WordLearningState[]` | 批量查询 |
| GET | `/api/word-states/due/list` | `?limit=50` | `WordLearningState[]` | 到期复习列表 |
| GET | `/api/word-states/stats/overview` | — (需认证) | `{ new, learning, reviewing, mastered }` | 状态统计概览 |
| POST | `/api/word-states/batch-update` | `{ updates: [{ wordId, state?, masteryLevel? }] }` | `{ updated }` | 批量更新 |
| POST | `/api/word-states/:word_id/mark-mastered` | — (需认证) | `WordLearningState` | 标记掌握 |
| POST | `/api/word-states/:word_id/reset` | — (需认证) | `WordLearningState` | 重置状态 |

**WordLearningState 模型**：
```typescript
interface WordLearningState {
  userId: string;
  wordId: string;
  state: "New" | "Learning" | "Reviewing" | "Mastered";
  masteryLevel: number;        // 0-1
  nextReviewDate?: string;     // ISO 8601
  halfLife: number;            // 小时
  correctStreak: number;
  totalAttempts: number;
  updatedAt: string;
}
```

### 3.9 AMAS `/api/amas`

| 方法 | 端点 | 请求体 | 响应 | 说明 |
|---|---|---|---|---|
| POST | `/api/amas/process-event` | `{ wordId, isCorrect, responseTime, sessionId?, isQuit?, ... }` | `ProcessResult` | 处理单个学习事件 |
| POST | `/api/amas/batch-process` | `{ events: [...] }` | `{ count, items: ProcessResult[] }` | 批量处理 |
| GET | `/api/amas/state` | — (需认证) | `UserState` | 查询当前用户 AMAS 状态（不触发处理） |
| GET | `/api/amas/strategy` | — (需认证) | `StrategyParams` | 获取当前推荐策略 |
| GET | `/api/amas/phase` | — (需认证) | `{ phase }` | 冷启动阶段 (Classify/Explore/Normal) |
| GET | `/api/amas/learning-curve` | — (需认证) | `{ curve: [{ date, total, correct, accuracy }] }` | 学习曲线 |
| GET | `/api/amas/intervention` | — (需认证) | `{ interventions: [{ type, message, severity }] }` | 干预建议 |
| POST | `/api/amas/reset` | — (需认证) | `{ reset: true }` | 重置 AMAS 状态 |
| GET | `/api/amas/mastery/evaluate` | `?wordId=xxx` (需认证) | `{ wordId, state, masteryLevel, correctStreak, totalAttempts, nextReviewDate }` | 掌握度评估 |
| GET | `/api/amas/config` | — (需 Admin) | `AMASConfig` | 获取配置 |
| PUT | `/api/amas/config` | `AMASConfig` (需 Admin) | `{ updated: true }` | 更新配置 |
| GET | `/api/amas/metrics` | — (需 Admin) | 算法指标快照 | 获取指标 |
| GET | `/api/amas/monitoring` | `?limit=50` (需 Admin) | 监控事件列表 | 获取监控 |

**ProcessResult 结构**：
```typescript
interface ProcessResult {
  sessionId: string;
  strategy: {
    difficulty: number;     // 0-1
    batchSize: number;
    newRatio: number;       // 0-1
    intervalScale: number;
    reviewMode: boolean;
  };
  explanation: {
    primaryReason: string;
    factors: { name: string; value: number; impact: string }[];
  };
  state: {
    attention: number;
    fatigue: number;
    motivation: number;
    confidence: number;
    lastActiveAt?: string;
    sessionEventCount: number;
    totalEventCount: number;
    createdAt: string;
  };
  wordMastery?: {
    wordId: string;
    memoryStrength: number;
    recallProbability: number;
    nextReviewIntervalSecs: number;
    masteryLevel: "New" | "Learning" | "Reviewing" | "Mastered";
  };
  reward: {
    value: number;
    components: {
      accuracyReward: number;
      speedReward: number;
      fatiguePenalty: number;
      frustrationPenalty: number;
    };
  };
  coldStartPhase?: "Classify" | "Explore" | "Exploit";
}
```

### 3.10 管理后台 `/api/admin`

#### 认证

| 方法 | 端点 | 请求体 | 响应 | 说明 |
|---|---|---|---|---|
| GET | `/api/admin/auth/status` | — | `{ initialized }` | 检查是否已初始化 Admin |
| POST | `/api/admin/auth/setup` | `{ email, password }` | `{ token, admin: { id, email } }` | 首次 Admin 创建 |
| POST | `/api/admin/auth/login` | `{ email, password }` | `{ token, admin: { id, email } }` | Admin 登录 |
| POST | `/api/admin/auth/logout` | — (需 Admin JWT) | `{ loggedOut: true }` | Admin 登出 |

#### 用户管理

| 方法 | 端点 | 说明 |
|---|---|---|
| GET | `/api/admin/users` | 用户列表（需 Admin JWT） |
| POST | `/api/admin/users/:id/ban` | 封禁用户 → `{ banned: true, userId }` |
| POST | `/api/admin/users/:id/unban` | 解封用户 → `{ banned: false, userId }` |
| GET | `/api/admin/stats` | 系统统计 → `{ users, words, records }` |

#### 数据分析

| 方法 | 端点 | 说明 |
|---|---|---|
| GET | `/api/admin/analytics/engagement` | `{ totalUsers, activeToday, retentionRate }` |
| GET | `/api/admin/analytics/learning` | `{ totalWords, totalRecords, totalCorrect, overallAccuracy }` |

#### 系统监控

| 方法 | 端点 | 说明 |
|---|---|---|
| GET | `/api/admin/monitoring/health` | `{ status, dbSizeBytes, uptime, version }` |
| GET | `/api/admin/monitoring/database` | `{ sizeOnDisk, treeCount, trees }` |

#### 广播 & 设置

| 方法 | 端点 | 说明 |
|---|---|---|
| POST | `/api/admin/broadcast` | `{ title, message }` → `{ sent }` |
| GET | `/api/admin/settings` | `{ maxUsers, registrationEnabled, maintenanceMode, defaultDailyWords }` |
| PUT | `/api/admin/settings` | 更新系统设置 |

**Admin 认证**：独立 JWT 密钥 (`ADMIN_JWT_SECRET`)，token_type 为 `"admin"`，独立 session 存储。

### 3.11 用户画像 `/api/user-profile`

| 方法 | 端点 | 说明 |
|---|---|---|
| GET/PUT | `/api/user-profile/reward` | 奖励偏好 (standard/explorer/achiever/social) |
| GET | `/api/user-profile/cognitive` | 认知画像（来自 AMAS） |
| GET | `/api/user-profile/learning-style` | 学习风格 VARK + 各维度分数 |
| GET | `/api/user-profile/chronotype` | 时间类型 (morning/evening/neutral) + 偏好时段 |
| GET/POST | `/api/user-profile/habit` | 习惯画像（偏好时段/会话中位数/每日频率） |
| POST | `/api/user-profile/avatar` | 头像上传（二进制） → `{ avatarUrl }` |

### 3.12 通知 & 偏好 `/api/notifications`

| 方法 | 端点 | 说明 |
|---|---|---|
| GET | `/api/notifications?limit=50&unreadOnly=false` | 通知列表 |
| PUT | `/api/notifications/:id/read` | 标记已读 |
| POST | `/api/notifications/read-all` | 全部已读 → `{ markedRead }` |
| GET | `/api/notifications/badges` | 徽章列表（first_word/streak_7/mastered_100） |
| GET/PUT | `/api/notifications/preferences` | 用户偏好 `{ theme, language, notificationEnabled, soundEnabled }` |

### 3.13 内容增强 `/api/content`

| 方法 | 端点 | 说明 |
|---|---|---|
| GET | `/api/content/etymology/:word_id` | 词源分析 `{ wordId, word, etymology, roots, generated }` |
| GET | `/api/content/semantic/search?query=xxx&limit=10` | 语义搜索 → `{ query, results, total, method }` |
| GET | `/api/content/word-contexts/:word_id` | 单词语境 `{ wordId, word, examples, contexts }` |
| GET/POST | `/api/content/morphemes/:word_id` | 词素拆解 `{ wordId, morphemes: [{ text, type, meaning }] }` |
| GET | `/api/content/confusion-pairs/:word_id` | 混淆词对 `{ wordId, confusionPairs }` |

### 3.14 健康检查 `/health`

| 方法 | 端点 | 说明 |
|---|---|---|
| GET | `/health` | 系统状态 `{ status, uptimeSecs, store: { healthy } }` |
| GET | `/health/live` | 存活探测 |
| GET | `/health/ready` | 就绪探测 |
| GET | `/health/database` | 数据库健康 `{ healthy, latencyUs, errorCount, consecutiveFailures }` |
| GET | `/health/metrics` | AMAS 算法指标 |

### 3.15 实时事件 `/api/realtime`

| 方法 | 端点 | 说明 |
|---|---|---|
| GET | `/api/realtime/events` | SSE 连接（需认证），支持 `amas_state` 事件 + keepalive |

### 3.16 V1 兼容路由 `/api/v1`

| 方法 | 端点 | 说明 |
|---|---|---|
| GET | `/api/v1/words`, `/api/v1/words/:id` | 单词查询 |
| GET/POST | `/api/v1/records` | 学习记录 |
| GET | `/api/v1/study-config` | 学习配置 |
| POST | `/api/v1/learning/session` | 创建会话 |

### 3.17 统一响应格式

```typescript
// 成功
{ success: true, data: T }

// 失败
{ success: false, error: string, code: string, message: string, traceId?: string }
```

### 3.18 静态文件服务

后端已实现 `ServeDir::new("static")` + SPA fallback (`static/index.html`)。前端构建产物输出到 `static/` 目录即可，同源部署，零 CORS。

---

## 四、项目结构

```
packages/frontend/
├── index.html                 # 用户端入口
├── admin.html                 # 管理后台入口
├── vite.config.ts
├── tsconfig.json
├── tailwind.config.ts
├── postcss.config.js
├── package.json
│
├── src/
│   ├── main.tsx               # 用户端入口
│   ├── admin-main.tsx         # 管理后台入口
│   ├── App.tsx                # 用户端根组件
│   ├── AdminApp.tsx           # 管理后台根组件
│   ├── index.css              # 全局样式 + CSS 变量 + Tailwind
│   │
│   ├── api/                   # API 层
│   │   ├── client.ts          # 基础 HTTP 客户端（fetch 封装、Token 注入、错误处理）
│   │   ├── auth.ts            # AuthClient
│   │   ├── words.ts           # WordClient
│   │   ├── records.ts         # RecordClient
│   │   ├── amas.ts            # AmasClient
│   │   ├── admin.ts           # AdminClient
│   │   ├── users.ts           # UserClient
│   │   ├── learning.ts        # LearningClient（会话/取词/调整）
│   │   ├── studyConfig.ts     # StudyConfigClient
│   │   ├── wordbooks.ts       # WordbookClient
│   │   ├── wordStates.ts      # WordStateClient
│   │   ├── userProfile.ts     # UserProfileClient
│   │   ├── notifications.ts   # NotificationClient
│   │   ├── content.ts         # ContentClient（词源/词素/混淆词）
│   │   └── types.ts           # API 请求/响应类型
│   │
│   ├── stores/                # 全局状态（SolidJS createSignal/createStore）
│   │   ├── auth.ts            # 认证状态（user, token, login, logout）
│   │   ├── theme.ts           # 主题（light/dark/system）
│   │   ├── ui.ts              # UI 状态（modal, toast, sidebar）
│   │   └── learning.ts        # 学习会话状态（前端队列管理，配合后端会话）
│   │
│   ├── queries/               # @tanstack/solid-query hooks
│   │   ├── words.ts
│   │   ├── records.ts
│   │   ├── stats.ts
│   │   ├── learning.ts        # 学习会话/取词 hooks
│   │   ├── wordbooks.ts       # 词书 hooks
│   │   ├── wordStates.ts      # 单词状态 hooks
│   │   ├── studyConfig.ts     # 学习配置 hooks
│   │   └── admin.ts
│   │
│   ├── components/            # 组件
│   │   ├── ui/                # 基础 UI 组件库
│   │   │   ├── Button.tsx
│   │   │   ├── Input.tsx
│   │   │   ├── Modal.tsx
│   │   │   ├── Card.tsx
│   │   │   ├── Toast.tsx
│   │   │   ├── Spinner.tsx
│   │   │   ├── Progress.tsx
│   │   │   ├── Table.tsx
│   │   │   ├── Badge.tsx
│   │   │   ├── Tabs.tsx
│   │   │   ├── Select.tsx
│   │   │   ├── Switch.tsx
│   │   │   ├── Skeleton.tsx
│   │   │   ├── Empty.tsx
│   │   │   └── Pagination.tsx
│   │   │
│   │   ├── layout/
│   │   │   ├── Navigation.tsx     # 全局导航栏
│   │   │   ├── PageLayout.tsx     # 页面布局容器
│   │   │   └── AdminLayout.tsx    # 管理后台布局
│   │   │
│   │   ├── auth/
│   │   │   ├── LoginForm.tsx
│   │   │   ├── RegisterForm.tsx
│   │   │   └── ProtectedRoute.tsx
│   │   │
│   │   ├── learning/
│   │   │   ├── WordCard.tsx           # 单词卡片（显示 text + pronunciation）
│   │   │   ├── ReverseWordCard.tsx     # 反向卡片（显示 meaning）
│   │   │   ├── TestOptions.tsx        # 四选一选项（干扰项由后端提供）
│   │   │   ├── FlashCard.tsx          # 闪卡（CSS 3D 翻转）
│   │   │   ├── LearningProgress.tsx   # 学习进度条
│   │   │   ├── SessionSummary.tsx     # 学习完成摘要
│   │   │   ├── LearningModeToggle.tsx # word-to-meaning / meaning-to-word 切换
│   │   │   └── AmasInsight.tsx        # AMAS 决策展示（策略+解释）
│   │   │
│   │   ├── words/
│   │   │   ├── WordList.tsx           # 单词列表
│   │   │   ├── WordForm.tsx           # 单词创建/编辑表单
│   │   │   ├── BatchImport.tsx        # 批量导入
│   │   │   └── WordDetail.tsx         # 单词详情（含学习状态 + 词源 + 词素 + 混淆词）
│   │   │
│   │   ├── wordbooks/
│   │   │   ├── WordbookList.tsx       # 词书列表
│   │   │   ├── WordbookForm.tsx       # 创建词书表单
│   │   │   └── WordbookDetail.tsx     # 词书详情（含单词管理）
│   │   │
│   │   ├── stats/
│   │   │   ├── StatsOverview.tsx      # 统计概览
│   │   │   ├── AccuracyChart.tsx      # 正确率图表
│   │   │   ├── StreakCard.tsx         # 连续学习天数
│   │   │   ├── UserStateRadar.tsx     # 用户认知状态雷达图
│   │   │   └── LearningCurve.tsx      # 学习曲线（来自 /api/amas/learning-curve）
│   │   │
│   │   ├── profile/
│   │   │   ├── ProfileCard.tsx
│   │   │   ├── PasswordChange.tsx
│   │   │   ├── ThemeToggle.tsx
│   │   │   ├── StudyConfigEditor.tsx  # 学习配置编辑器（每日目标/词书选择/学习模式）
│   │   │   ├── CognitiveProfile.tsx   # 认知画像展示
│   │   │   └── AvatarUpload.tsx       # 头像上传
│   │   │
│   │   ├── notifications/
│   │   │   ├── NotificationBell.tsx   # 通知铃铛（未读计数）
│   │   │   ├── NotificationList.tsx   # 通知列表
│   │   │   └── BadgeGrid.tsx          # 徽章展示
│   │   │
│   │   └── admin/
│   │       ├── UserTable.tsx          # 用户管理表格
│   │       ├── UserActions.tsx        # 封禁/解封操作
│   │       ├── SystemStats.tsx        # 系统统计卡片
│   │       ├── AmasConfigEditor.tsx   # AMAS 配置编辑器
│   │       ├── AmasMetrics.tsx        # 算法指标展示
│   │       ├── MonitoringEvents.tsx   # 监控事件列表
│   │       ├── AdminLogin.tsx         # Admin 登录
│   │       ├── AdminSetup.tsx         # 首次 Admin 创建
│   │       ├── AnalyticsDashboard.tsx # 数据分析面板
│   │       ├── SystemHealth.tsx       # 系统健康监控
│   │       ├── DatabaseInfo.tsx       # 数据库信息
│   │       ├── BroadcastForm.tsx      # 广播消息
│   │       └── SystemSettings.tsx     # 系统设置
│   │
│   ├── pages/                 # 页面组件（路由对应）
│   │   ├── HomePage.tsx
│   │   ├── LoginPage.tsx
│   │   ├── RegisterPage.tsx
│   │   ├── LearningPage.tsx       # 核心学习页
│   │   ├── FlashcardPage.tsx
│   │   ├── VocabularyPage.tsx     # 词库（单词列表 + CRUD）
│   │   ├── WordbookPage.tsx       # 词书管理
│   │   ├── StatisticsPage.tsx
│   │   ├── ProfilePage.tsx
│   │   ├── HistoryPage.tsx        # 学习历史记录
│   │   ├── NotificationsPage.tsx  # 通知页
│   │   │
│   │   └── admin/
│   │       ├── AdminLoginPage.tsx
│   │       ├── AdminSetupPage.tsx    # 首次初始化
│   │       ├── AdminDashboard.tsx
│   │       ├── UserManagementPage.tsx
│   │       ├── AmasConfigPage.tsx
│   │       ├── MonitoringPage.tsx
│   │       ├── AnalyticsPage.tsx     # 数据分析
│   │       └── SettingsPage.tsx      # 系统设置
│   │
│   ├── lib/
│   │   ├── WordQueueManager.ts    # 前端单词队列管理器（配合后端会话/取词 API）
│   │   ├── queryClient.ts         # TanStack Query 客户端配置
│   │   ├── storage.ts             # localStorage 封装
│   │   └── token.ts               # Token 管理（存储/过期检查/自动刷新）
│   │
│   ├── utils/
│   │   ├── cn.ts                  # Tailwind class merge 工具
│   │   └── formatters.ts          # 日期/数字格式化
│   │
│   └── types/
│       ├── index.ts               # 所有类型统一导出
│       ├── api.ts                 # API 响应类型
│       ├── word.ts                # Word 相关
│       ├── record.ts              # LearningRecord 相关
│       ├── amas.ts                # AMAS 相关
│       ├── user.ts                # User 相关
│       ├── wordbook.ts            # Wordbook 相关
│       ├── wordState.ts           # WordLearningState 相关
│       ├── studyConfig.ts         # StudyConfig 相关
│       ├── learning.ts            # LearningSession 相关
│       └── notification.ts        # Notification / Badge 相关
```

---

## 五、开发阶段

### 阶段 0：项目初始化（0.5 天）

- [ ] 初始化 SolidJS + Vite + TypeScript 项目
- [ ] 配置 Tailwind CSS 4
- [ ] 配置多入口（index.html + admin.html）
- [ ] 配置 Vite 代理 → `localhost:3000`
- [ ] 配置路径别名 `@/` → `./src/`
- [ ] 搭建基础目录结构
- [ ] 设计 CSS 变量体系（Surface/Content/Accent/Status 颜色）
- [ ] 实现 FOUC 防护（index.html 同步 script）
- [ ] 配置构建输出到 `static/` 目录（对接后端 ServeDir）

### 阶段 1：基础 UI 组件库（1.5 天）

搭建可复用的基础组件库，所有组件遵循 G3 设计语言风格：

- [ ] `Button`（7 变体：primary/secondary/outline/ghost/danger/success/warning × 5 尺寸）
- [ ] `Input`（含错误状态、图标支持）
- [ ] `Modal`（带遮罩、关闭动画）
- [ ] `Card`（elevated/outlined/filled/glass 变体）
- [ ] `Toast` + `ToastProvider`（4 类型：success/error/warning/info）
- [ ] `Spinner` / `Skeleton`（加载状态）
- [ ] `Progress`（进度条 + 圆形进度）
- [ ] `Table`（排序、分页支持）
- [ ] `Select` / `Switch` / `Tabs`
- [ ] `Badge` / `Tag`
- [ ] `Empty`（空状态占位）
- [ ] `Pagination`

### 阶段 2：API 层 + 认证系统（1.5 天）

- [ ] 实现 `BaseClient`（fetch 封装、Token 注入、超时控制、错误处理、401 拦截）
- [ ] 实现 `TokenManager`（localStorage 存储、过期检查、自动刷新调用 `/api/auth/refresh`）
- [ ] 实现各模块 Client（AuthClient、WordClient、RecordClient、AmasClient、UserClient、AdminClient、LearningClient、StudyConfigClient、WordbookClient、WordStateClient、UserProfileClient、NotificationClient、ContentClient）
- [ ] 实现 Auth Store（createSignal：user/token/loading、login/register/logout）
- [ ] 实现乐观加载策略（启动时从 localStorage 读取缓存 → 后台静默验证）
- [ ] 实现 `ProtectedRoute` 组件
- [ ] 实现 TanStack Solid Query 配置
- [ ] 实现所有模块的 Query/Mutation hooks

### 阶段 3：路由 + 布局 + 主题（0.5 天）

**用户端路由**：

| 路径 | 页面 |
|---|---|
| `/` | HomePage（仪表盘或登录引导） |
| `/login` | LoginPage |
| `/register` | RegisterPage |
| `/learning` | LearningPage |
| `/flashcard` | FlashcardPage |
| `/vocabulary` | VocabularyPage（单词 CRUD） |
| `/wordbooks` | WordbookPage（词书管理） |
| `/statistics` | StatisticsPage |
| `/history` | HistoryPage（学习记录） |
| `/profile` | ProfilePage |
| `/notifications` | NotificationsPage |

**管理后台路由**（独立入口 admin.html）：

| 路径 | 页面 |
|---|---|
| `/admin/login` | AdminLoginPage |
| `/admin/setup` | AdminSetupPage（首次初始化） |
| `/admin` | AdminDashboard |
| `/admin/users` | UserManagementPage |
| `/admin/amas-config` | AmasConfigPage |
| `/admin/monitoring` | MonitoringPage |
| `/admin/analytics` | AnalyticsPage |
| `/admin/settings` | SettingsPage |

- [ ] 配置 @solidjs/router
- [ ] 实现 `Navigation` 组件（响应式导航栏）
- [ ] 实现 `PageLayout` 组件
- [ ] 实现 `AdminLayout` 组件
- [ ] 实现 `ThemeToggle`（light/dark/system 三态切换）
- [ ] 实现 Theme Store（CSS 变量切换 + localStorage 持久化 + prefers-color-scheme 监听）
- [ ] 实现懒加载（SolidJS `lazy()`）

### 阶段 4：认证页面（0.5 天）

- [ ] LoginPage（邮箱+密码表单、错误提示、登录成功跳转）
- [ ] RegisterPage（邮箱+用户名+密码表单、验证规则、注册成功自动登录）
- [ ] ProfilePage（显示用户信息、修改用户名、修改密码、认知画像、学习风格、头像上传）
- [ ] 登录后预加载常用数据

### 阶段 5：单词管理 + 词书系统（1.5 天）

- [ ] VocabularyPage — 单词列表（分页、搜索过滤、总数显示）
- [ ] WordForm — 创建/编辑单词表单
- [ ] WordDetail — 单词详情页（含学习状态、词源分析、词素拆解、混淆词对）
- [ ] BatchImport — 批量创建（支持 CSV 粘贴、JSON 输入、URL 导入）
- [ ] 单词删除功能
- [ ] WordbookPage — 词书列表（系统词书 + 用户词书）
- [ ] WordbookForm — 创建词书
- [ ] WordbookDetail — 词书详情（浏览/添加/移除单词）
- [ ] Query hooks：`useWords()`、`useWord(id)`、`useWordbooks()`、`useWordbookWords(id)`
- [ ] Mutation hooks：`useCreateWord()`、`useUpdateWord()`、`useDeleteWord()`、`useBatchCreate()`、`useImportUrl()`、`useCreateWordbook()`

### 阶段 6：核心学习页面（2 天）⭐ 最重要

#### 6.1 学习流程（后端驱动）

学习流程由后端会话 + 取词 API 驱动，前端负责 UI 展示和答题交互：

```
POST /api/learning/session → 获取 sessionId
  ↓
POST /api/learning/study-words → 获取带策略的学习单词（后端返回含干扰项的完整单词列表）
  ↓
前端 WordQueueManager 管理 Active/Mastered 队列
  ↓
用户答题 → POST /api/records（后端自动更新 word_states + session 计数 + AMAS）
  ↓ 返回 amasResult
应用 AMAS 策略调整
  ↓
需要补充单词时 → POST /api/learning/next-words（后端返回下一批含干扰项的单词）
  ↓
完成条件检查 → SessionSummary
```

#### 6.2 WordQueueManager（精简版）

前端队列管理器配合后端 API，职责简化为：

```
后端返回的 study-words
    ↓
Active 队列（batchSize 个活跃单词）
    ↓ 掌握
Mastered 队列（已掌握）
    ↓ Active 不足时
POST /api/learning/next-words 补充
```

- [ ] 实现双队列数据结构（Active + Mastered）
- [ ] 实现选词策略（错误优先、间隔避免）
- [ ] 实现答题记录追踪（连续正确计数、错误计数）
- [ ] 实现 AMAS 策略应用（从 amasResult.strategy 读取 batchSize、difficulty、newRatio）
- [ ] 实现自动补词（Active 队列不足时调 next-words）
- [ ] 实现状态持久化到 localStorage（断线恢复）

#### 6.3 学习页面

- [ ] LearningPage 整体布局
- [ ] WordCard（显示 text + pronunciation，发音按钮预留）
- [ ] ReverseWordCard（显示 meaning，选择正确单词）
- [ ] TestOptions（四选一，干扰项由后端提供，正确/错误视觉反馈动画）
- [ ] LearningProgress（进度条：已掌握数/目标数，数据来自后端 session）
- [ ] LearningModeToggle（word-to-meaning / meaning-to-word 切换，持久化到 localStorage）
- [ ] AmasInsight（展示当前策略和决策解释，可折叠）
- [ ] StudyConfigEditor（学习前选择词书、每日目标、学习模式）

#### 6.4 答题流程

```
显示单词卡片 + 4 个选项（1 正确 + 3 干扰项，均来自后端）
  ↓ 用户选择答案
显示反馈（绿色正确/红色错误，2秒）
  ↓ 同时异步
POST /api/records（提交答题记录，获得 amasResult，后端自动更新 word_states + session）
  ↓
应用 AMAS 策略到 WordQueueManager
  ↓
检查完成条件（掌握数 ≥ 目标 || 题数 ≥ 上限 || 队列空）
  ↓ 未完成        ↓ 完成
显示下一题       SessionSummary（统计摘要）
```

- [ ] 实现答案提交逻辑（乐观更新 UI → POST → 应用策略）
- [ ] 实现完成条件检查（数据来自后端 session 的 actualMasteryCount/targetMasteryCount）
- [ ] 实现 SessionSummary 页面（总答题数、正确率、掌握数、AMAS 状态展示）
- [ ] 实现 beforeunload 处理（页面关闭时缓存进度 + sync-progress）

#### 6.5 闪记模式

- [ ] FlashcardPage
- [ ] FlashCard 组件（CSS 3D 翻转：正面 text/pronunciation，反面 meaning）
- [ ] 快捷键支持（空格翻转、→/1 认识、←/2 不认识）

### 阶段 7：数据统计（1 天）

- [ ] StatisticsPage
- [ ] StatsOverview（总学习单词、总记录数、连续天数、正确率 — 来自 `/api/users/me/stats`）
- [ ] StreakCard（连续学习天数展示 — 来自 `/api/records/statistics/enhanced` 的 streak）
- [ ] AccuracyChart（正确率可视化 — 来自 `/api/records/statistics/enhanced` 的 daily 数据）
- [ ] LearningCurve（学习曲线 — 来自 `/api/amas/learning-curve`）
- [ ] UserStateRadar（AMAS 用户状态雷达图 — 来自 `/api/amas/state`）
- [ ] WordStateOverview（单词状态分布 — 来自 `/api/word-states/stats/overview`）
- [ ] HistoryPage（学习记录列表，来自 `/api/records?offset=x`，分页）

### 阶段 8：首页仪表盘（0.5 天）

- [ ] HomePage
  - 已登录：仪表盘（今日学习概览 via `study-config/progress`、统计摘要、快速开始学习按钮、AMAS 状态卡片、干预建议 via `amas/intervention`）
  - 未登录：欢迎页（特性介绍、登录/注册入口）
- [ ] DailyMissionCard（学习目标进度 — 来自 `/api/study-config/progress`）
- [ ] QuickStartButton（直接进入学习）
- [ ] NotificationBell（导航栏通知铃铛 + 未读计数）

### 阶段 9：管理后台（2 天）

**独立入口（admin.html → admin-main.tsx → AdminApp.tsx）**

- [ ] AdminSetupPage（首次访问检测 `/api/admin/auth/status`，未初始化则引导创建 Admin）
- [ ] AdminLoginPage（管理员登录 `/api/admin/auth/login`）
- [ ] AdminLayout（侧边栏导航 + 内容区）
- [ ] AdminDashboard
  - SystemStats（用户数/单词数/记录数 — `/api/admin/stats`）
  - SystemHealth（系统健康 — `/api/admin/monitoring/health`）
  - DatabaseInfo（数据库信息 — `/api/admin/monitoring/database`）
- [ ] UserManagementPage
  - UserTable（用户列表 — `/api/admin/users`）
  - UserActions（封禁/解封 — `/api/admin/users/:id/ban|unban`）
- [ ] AmasConfigPage
  - AmasConfigEditor（JSON 编辑器 — GET/PUT `/api/amas/config`）
  - AmasMetrics（算法指标展示 — `/api/amas/metrics`）
  - MonitoringEvents（监控事件列表 — `/api/amas/monitoring`）
- [ ] AnalyticsPage
  - EngagementAnalytics（用户活跃 — `/api/admin/analytics/engagement`）
  - LearningAnalytics（学习数据 — `/api/admin/analytics/learning`）
- [ ] SettingsPage
  - SystemSettings（系统设置 — GET/PUT `/api/admin/settings`）
  - BroadcastForm（广播消息 — POST `/api/admin/broadcast`）

**注意**：管理员认证体系独立于用户认证。需要独立的 AdminAuthStore 和 AdminProtectedRoute。

### 阶段 10：打磨完善（1 天）

- [ ] 响应式适配（移动端导航汉堡菜单、卡片自适应宽度）
- [ ] 骨架屏加载状态
- [ ] 错误边界组件（ErrorBoundary）
- [ ] 404 / 403 页面
- [ ] 离线检测提示（OfflineIndicator）
- [ ] 主题切换过渡动画
- [ ] 键盘快捷键（学习页面：1-4 选择答案）
- [ ] 全局 Toast 通知整合（API 错误统一处理）
- [ ] SSE 连接（`/api/realtime/events` — 监听 amas_state 变更）
- [ ] 通知系统集成（NotificationBell 全局组件）

---

## 六、后端缺口清单

> 以下是前端需要但后端**尚未实现**的功能。

### 阻塞项（P0）

| 功能 | 说明 | 复杂度 |
|---|---|---|
| **干扰项生成** | `study-words` 和 `next-words` 端点需要为每个目标单词返回 3 个干扰项（相近难度的其他单词的 meaning 或 text），当前仅返回目标单词列表 | 中 |

### 增强项（P1）

| 功能 | 说明 | 复杂度 |
|---|---|---|
| 学习会话完成 | `POST /api/learning/session/complete` — 显式标记会话完成（当前只有创建/恢复/同步，无正式关闭） | 低 |
| 学习模式感知 | `study-words` 应根据 `studyMode`（MASTERY/REVIEW/MIXED）调整选词策略 | 低 |
| 今日已学判定 | `study-config/today-words` 应排除今日已答过的单词 | 低 |

### 远期（P2+）

| 功能 | 说明 | 复杂度 |
|---|---|---|
| 发音 TTS | 单词发音 API 或集成第三方 TTS | 中 |
| 邮件集成 | forgot-password 对接实际邮件发送 | 中 |
| LLM 顾问 | LLM 建议 HTTP 路由 | 中 |

---

## 七、关键设计决策

### 7.1 认证架构

```
用户端：
  Bearer Token (JWT) → localStorage('auth_token')
  登录 → 获得 token → 存储 → 后续请求自动注入
  自动刷新 → 过期前 5 分钟调用 /api/auth/refresh
  401 → 清除 token → 跳转 /login

管理后台（独立入口）：
  Admin JWT → localStorage('admin_token')
  首次访问 → GET /api/admin/auth/status 检查初始化
    → 未初始化 → AdminSetupPage → POST /api/admin/auth/setup
    → 已初始化 → AdminLoginPage → POST /api/admin/auth/login
  独立 Admin JWT (ADMIN_JWT_SECRET)，token_type: "admin"
```

### 7.2 学习会话 = 后端管理

后端有完整的 LearningSession 实体：

```
1. POST /api/learning/session → 创建新会话或恢复活跃会话（自动关闭其他活跃会话）
   ↓ 返回 { sessionId, status, resumed }
2. POST /api/learning/study-words → 后端基于 AMAS 策略 + 词书配置 + word_states 选词
   ↓ 返回 { words, strategy }
3. 每次答题 POST /api/records 带 sessionId
   ↓ 后端自动递增 session.totalQuestions / actualMasteryCount，自动更新 word_states
4. 需要补词 → POST /api/learning/next-words（排除已有、标记已掌握）
5. 前端 sync-progress → POST /api/learning/sync-progress（同步额外字段）
```

前端 `WordQueueManager` 的职责简化为：
- 维护 Active/Mastered 双队列（本地 UI 状态）
- 选择下一个展示的单词（错误优先、间隔避免）
- Active 不足时调用 `next-words` 补充
- 缓存到 localStorage 用于断线恢复

### 7.3 答题选项生成（后端提供干扰项）

**后端需新增支持**（见缺口清单 P0）：

`study-words` 和 `next-words` 的响应需要扩展为：

```typescript
interface StudyWord {
  word: Word;                    // 目标单词
  distractors: DistractorSet;    // 干扰项
}

interface DistractorSet {
  meanings: string[];   // 3 个干扰 meaning（word-to-meaning 模式）
  texts: string[];      // 3 个干扰 text（meaning-to-word 模式）
}
```

干扰项选择策略（后端实现）：
1. 相近难度（difficulty 差值 < 0.3）
2. 不同含义（排除正确答案）
3. 来自同一词书优先
4. 随机排列

前端只需将正确答案 + 3 个干扰项合并、随机排序后展示。

### 7.4 AMAS 集成方式

推荐路径：**POST `/api/records`** 一次请求同时完成：
1. 创建学习记录
2. AMAS 引擎处理 → 返回 `amasResult`
3. 自动更新 `word_learning_states`（掌握度/状态/复习间隔）
4. 自动更新 `learning_session` 计数

备用路径：`POST /api/amas/process-event`（不创建记录，不更新状态）

独立查询端点：
- `GET /api/amas/state` — 查看用户状态（不触发处理）
- `GET /api/amas/strategy` — 查看当前推荐策略
- `GET /api/amas/phase` — 查看冷启动阶段
- `GET /api/amas/learning-curve` — 学习曲线数据
- `GET /api/amas/intervention` — 干预建议

### 7.5 词书 + 学习配置

```
用户首次使用：
  1. 浏览系统词书 → GET /api/wordbooks/system
  2. 选择词书 → PUT /api/study-config（设置 selectedWordbookIds）
  3. 设置每日目标 → PUT /api/study-config（设置 dailyWordCount / dailyMasteryTarget）
  4. 开始学习 → 后端 study-words 自动从选中词书取词

用户也可以：
  - 创建自定义词书 → POST /api/wordbooks
  - 向词书添加单词 → POST /api/wordbooks/:id/words
  - 批量导入 → POST /api/words/batch 或 /api/words/import-url
```

---

## 八、时间估算

| 阶段 | 内容 | 预估 |
|---|---|---|
| 0 | 项目初始化 | 0.5 天 |
| 1 | UI 组件库 | 1.5 天 |
| 2 | API 层 + 认证 | 1.5 天 |
| 3 | 路由 + 布局 + 主题 | 0.5 天 |
| 4 | 认证页面 | 0.5 天 |
| 5 | 单词管理 + 词书系统 | 1.5 天 |
| 6 | **核心学习页面** | **2 天** |
| 7 | 数据统计 | 1 天 |
| 8 | 首页仪表盘 | 0.5 天 |
| 9 | 管理后台 | 2 天 |
| 10 | 打磨完善 | 1 天 |
| **总计** | | **~12 天** |

---

## 九、已确认决策

| # | 问题 | 决策 |
|---|---|---|
| 1 | Admin 登录端点 | **已实现**：`/api/admin/auth/status`（检查初始化）→ `/api/admin/auth/setup`（首次创建）→ `/api/admin/auth/login`（登录）→ `/api/admin/auth/logout`（登出） |
| 2 | 单词数据来源 | **GitHub 自动导入**：前端提供 GitHub 词库 URL 输入，调用 `/api/words/import-url` 后端代理抓取。也支持 JSON/CSV 粘贴后调用 `/api/words/batch`。 |
| 3 | 干扰项策略 | **后端提供**：`study-words` / `next-words` 端点为每个目标单词返回 3 个干扰项。（**待后端实现**，见缺口清单 P0） |
| 4 | AMAS 调用路径 | **只走 POST `/api/records`**，其返回的 `amasResult` 即为 AMAS 决策结果，同时自动更新 word_states 和 session。独立查询用 `GET /api/amas/state` 等端点。 |
| 5 | 部署方式 | **已实现：后端静态服务**。前端 `dist/` → `static/` 由 Axum `ServeDir` 托管，SPA fallback 到 `static/index.html`，同源部署，零 CORS。 |
| 6 | 学习会话管理 | **后端驱动**：后端维护 LearningSession 实体。前端通过 `/api/learning/session` 创建/恢复会话，通过 `/api/learning/study-words` 获取学习单词，答题时后端自动更新会话计数。 |
| 7 | 词书系统 | **已实现**：系统词书 + 用户词书，与学习配置（study-config）联动，study-words 自动从选中词书取词。 |

---

## 十、GitHub 词库导入方案

### 后端代理方式（推荐）

后端已实现 `POST /api/words/import-url`，支持从外部 URL 抓取内容并解析导入。

**用户流程**：
1. 用户在 VocabularyPage 点击「从 URL 导入」
2. 输入文件 URL（GitHub raw、其他 CDN 等）
3. 调用 `POST /api/words/import-url`（后端代理抓取，无跨域问题）
4. 后端解析格式（tab 分隔 / " - " 分隔），创建单词
5. 返回 `{ imported, items }` 显示导入结果

**支持的格式**（后端 import-url）：
```
# 注释行（# 开头）会被跳过

# 格式 1：Tab 分隔
abandon	放弃
resilient	有弹性的

# 格式 2：" - " 分隔
abandon - 放弃
resilient - 有弹性的
```

### 前端解析方式（JSON / CSV）

前端 BatchImport 组件也支持用户直接粘贴数据，前端解析后调用 `/api/words/batch`：

```typescript
// JSON 数组
[
  { "text": "abandon", "meaning": "放弃", "pronunciation": "/əˈbændən/", "tags": ["CET4"] },
  ...
]

// CSV
text,meaning,pronunciation,partOfSpeech,tags
abandon,放弃,/əˈbændən/,v,CET4
```

**前端实现**：
- `BatchImport.tsx` 组件：URL 输入 + 文本粘贴 + 格式选择 + 预览 + 确认
- 分批提交（每批 50 个，避免请求过大）
- 进度展示 + 错误处理（跳过格式错误的行）
