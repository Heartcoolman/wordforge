# 学习 API

## 学习会话 `/api/learning`

| 方法 | 端点 | 说明 |
|------|------|------|
| POST | `/api/learning/session` | 创建/恢复学习会话 |
| POST | `/api/learning/study-words` | 获取学习单词（基于 AMAS 策略） |
| POST | `/api/learning/next-words` | 获取下一批单词 |
| POST | `/api/learning/adjust-words` | 动态调整策略 |
| POST | `/api/learning/sync-progress` | 同步会话进度 |

### 学习流程

```
POST /api/learning/session → 获取 sessionId
  ↓
POST /api/learning/study-words → 获取带策略的学习单词
  ↓
用户答题 → POST /api/records（自动更新状态）
  ↓
需要补充 → POST /api/learning/next-words
  ↓
完成 → SessionSummary
```

## 学习记录 `/api/records`

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/api/records` | 获取学习记录（`?limit=50&offset=0`） |
| POST | `/api/records` | 提交答题记录 + AMAS 处理 |
| POST | `/api/records/batch` | 批量提交 |
| GET | `/api/records/statistics` | 基础统计 |
| GET | `/api/records/statistics/enhanced` | 增强统计（含每日分组 + 连续天数） |

### 提交答题请求体

```typescript
interface CreateRecordRequest {
  wordId: string;
  isCorrect: boolean;
  responseTimeMs: number;
  sessionId?: string;
  isQuit?: boolean;
  dwellTimeMs?: number;
  pauseCount?: number;
  hintUsed?: boolean;
}
```

POST `/api/records` 是学习流程的**核心端点**，后端同时完成：
1. 创建学习记录
2. 调用 AMAS 引擎处理，返回 `amasResult`
3. 自动更新 `word_learning_states`
4. 自动更新 `learning_session` 计数

## 学习配置 `/api/study-config`

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/api/study-config` | 获取学习配置 |
| PUT | `/api/study-config` | 更新配置 |
| GET | `/api/study-config/today-words` | 今日学习单词 |
| GET | `/api/study-config/progress` | 学习进度 |

### StudyConfig 模型

```typescript
interface StudyConfig {
  userId: string;
  selectedWordbookIds: string[];
  dailyWordCount: number;       // 1-200，默认 20
  studyMode: "MASTERY" | "REVIEW" | "MIXED";
  dailyMasteryTarget: number;   // 1-100
}
```

## AMAS API `/api/amas`

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/api/amas/state` | 用户 AMAS 状态 |
| GET | `/api/amas/strategy` | 当前推荐策略 |
| GET | `/api/amas/phase` | 冷启动阶段 |
| GET | `/api/amas/learning-curve` | 学习曲线 |
| GET | `/api/amas/intervention` | 干预建议 |
| POST | `/api/amas/reset` | 重置 AMAS 状态 |
| GET | `/api/amas/mastery/evaluate` | 掌握度评估（`?wordId=xxx`） |
