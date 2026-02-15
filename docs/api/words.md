# 单词管理 API

## 单词 `/api/words`

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/api/words` | 单词列表（`?limit=20&offset=0&search=xxx`） |
| GET | `/api/words/:id` | 单词详情 |
| POST | `/api/words` | 创建单词 |
| PUT | `/api/words/:id` | 更新单词 |
| DELETE | `/api/words/:id` | 删除单词 |
| POST | `/api/words/batch` | 批量创建 |
| GET | `/api/words/count` | 单词总数 |
| POST | `/api/words/import-url` | URL 导入 |

### Word 模型

```typescript
interface Word {
  id: string;
  text: string;
  meaning: string;
  pronunciation?: string;
  partOfSpeech?: string;
  difficulty: number;    // 0-1
  examples: string[];
  tags: string[];
  createdAt: string;     // ISO 8601
}
```

## 词书 `/api/wordbooks`

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/api/wordbooks/system` | 系统词书列表 |
| GET | `/api/wordbooks/user` | 用户词书列表（需认证） |
| POST | `/api/wordbooks` | 创建用户词书 |
| GET | `/api/wordbooks/:id/words` | 词书内单词（分页） |
| POST | `/api/wordbooks/:id/words` | 向词书添加单词 |
| DELETE | `/api/wordbooks/:id/words/:word_id` | 从词书移除单词 |

### Wordbook 模型

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

## 单词学习状态 `/api/word-states`

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/api/word-states/:word_id` | 查询单词学习状态 |
| POST | `/api/word-states/batch` | 批量查询 |
| GET | `/api/word-states/due/list` | 到期复习列表 |
| GET | `/api/word-states/stats/overview` | 状态统计概览 |
| POST | `/api/word-states/:word_id/mark-mastered` | 标记掌握 |
| POST | `/api/word-states/:word_id/reset` | 重置状态 |

### WordLearningState 模型

```typescript
interface WordLearningState {
  userId: string;
  wordId: string;
  state: "New" | "Learning" | "Reviewing" | "Mastered";
  masteryLevel: number;        // 0-1
  nextReviewDate?: string;
  halfLife: number;            // 小时
  correctStreak: number;
  totalAttempts: number;
  updatedAt: string;
}
```
