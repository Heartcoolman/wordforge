# 学习模块接口

学习模块包含两组路由：学习会话管理（`/api/learning`）和学习配置（`/api/study-config`）。所有接口均需认证。

---

## 学习会话（/api/learning）

### 创建或恢复会话

**POST** `/api/learning/session`

创建新的学习会话，若已有活跃会话则直接恢复。新会话创建时会查询最近 2 小时内完成的会话，生成跨会话提示（`crossSessionHint`），用于难度衔接。

**请求体**（可选）：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `targetMasteryCount` | `u32` | 否 | 本次目标掌握数，默认取学习配置中的 `dailyMasteryTarget` |

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `sessionId` | `string` | 会话 ID |
| `status` | `string` | `"active"` / `"completed"` / `"abandoned"` |
| `resumed` | `bool` | 是否为恢复的已有会话 |
| `targetMasteryCount` | `u32` | 目标掌握数 |
| `crossSessionHint` | `object?` | 跨会话提示，仅新会话且存在近期完成会话时返回 |

`crossSessionHint` 结构：

| 字段 | 类型 | 说明 |
|------|------|------|
| `prevAccuracy` | `f64` | 上次会话正确率 |
| `prevMasteredCount` | `usize` | 上次掌握单词数 |
| `gapMinutes` | `i64` | 距上次会话间隔（分钟） |
| `suggestedDifficulty` | `f64` | 建议难度 |
| `errorProneWordIds` | `string[]` | 上次易错单词 ID 列表 |
| `recentlyMasteredWordIds` | `string[]` | 上次已掌握单词 ID 列表 |

---

### 获取学习单词

**GET** `/api/learning/study-words`

基于 AMAS 策略和用户已选词书，计算并返回当前批次的学习单词。使用 `word_selector` 评分排序选词。

**请求参数**：无

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `words` | `WordPublic[]` | 单词列表 |
| `strategy` | `object` | 当前学习策略参数 |

`strategy` 结构：

| 字段 | 类型 | 说明 |
|------|------|------|
| `difficultyRange` | `[f64, f64]` | 难度范围（基准 ±0.2） |
| `newRatio` | `f64` | 新词占比 |
| `batchSize` | `u32` | 批次大小 |

---

### 获取下一批单词

**POST** `/api/learning/next-words`

根据已学单词和会话表现，动态调整策略后获取下一批单词。支持冲刺模式（接近目标时提高新词比例）。

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `excludeWordIds` | `string[]` | 是 | 需排除的已学单词 ID |
| `masteredWordIds` | `string[]` | 否 | 本次已掌握的单词 ID，会被标记为 Mastered 状态 |
| `sessionPerformance` | `object` | 否 | 会话表现数据，用于动态调整策略 |

`sessionPerformance` 结构：

| 字段 | 类型 | 说明 |
|------|------|------|
| `recentAccuracy` | `f64` | 近期正确率 |
| `masteredCount` | `u32` | 已掌握数量 |
| `targetMasteryCount` | `u32` | 目标掌握数量 |
| `errorProneWordIds` | `string[]` | 易错单词 ID 列表 |

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `words` | `WordPublic[]` | 下一批单词 |
| `batchSize` | `usize` | 批次大小 |

---

### 调整学习策略

**POST** `/api/learning/adjust-words`

根据近期表现或用户状态动态调整学习策略参数。支持多种用户状态关键词。

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `recentPerformance` | `f64` | 否 | 近期表现（0.0~1.0），影响难度和新词比例 |
| `userState` | `string` | 否 | 用户状态关键词 |

`userState` 支持的值：

| 值 | 效果 |
|------|------|
| `focused` / `engaged` / `confident` | 提高难度和新词比例 |
| `tired` / `fatigued` / `frustrated` / `distracted` | 降低难度、新词比例，缩小批次 |
| `review` | 切换为纯复习模式（新词比例为 0） |
| `sprint` | 冲刺模式，大幅提高新词比例 |

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `adjustedStrategy` | `object` | 调整后的策略参数 |

`adjustedStrategy`（`StrategyParams`）结构：

| 字段 | 类型 | 说明 |
|------|------|------|
| `difficulty` | `f64` | 难度系数 |
| `batchSize` | `u32` | 批次大小 |
| `newRatio` | `f64` | 新词占比 |
| `intervalScale` | `f64` | 复习间隔倍率 |
| `reviewMode` | `bool` | 是否为复习模式 |

---

### 同步学习进度

**POST** `/api/learning/sync-progress`

同步会话的题目数和上下文切换次数，仅递增不递减。

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `sessionId` | `string` | 是 | 会话 ID |
| `totalQuestions` | `u32` | 否 | 总题目数（仅当大于当前值时更新） |
| `contextShifts` | `u32` | 否 | 上下文切换次数（仅当大于当前值时更新） |

**响应体**：`LearningSession` 对象。

`LearningSession` 结构：

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | `string` | 会话 ID |
| `userId` | `string` | 用户 ID |
| `status` | `string` | `"active"` / `"completed"` / `"abandoned"` |
| `targetMasteryCount` | `u32` | 目标掌握数 |
| `totalQuestions` | `u32` | 总题目数 |
| `actualMasteryCount` | `u32` | 实际掌握数 |
| `contextShifts` | `u32` | 上下文切换次数 |
| `createdAt` | `DateTime` | 创建时间 |
| `updatedAt` | `DateTime` | 更新时间 |
| `summary` | `SessionSummary?` | 会话摘要（完成后才有） |
| `correctCount` | `u32` | 正确计数 |
| `totalCount` | `u32` | 总计数 |

---

### 完成会话

**POST** `/api/learning/complete-session`

完成学习会话，计算正确率并生成摘要，同时更新用户的时间段学习画像。

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `sessionId` | `string` | 是 | 会话 ID |
| `masteredWordIds` | `string[]` | 是 | 已掌握单词 ID 列表 |
| `errorProneWordIds` | `string[]` | 是 | 易错单词 ID 列表 |
| `avgResponseTimeMs` | `i64` | 是 | 平均响应时间（毫秒） |

**响应体**：`LearningSession` 对象（含 `summary`）。

`SessionSummary` 结构：

| 字段 | 类型 | 说明 |
|------|------|------|
| `accuracy` | `f64` | 正确率 |
| `avgResponseTimeMs` | `i64` | 平均响应时间（毫秒） |
| `masteredWordIds` | `string[]` | 已掌握单词 ID |
| `errorProneWordIds` | `string[]` | 易错单词 ID |
| `durationSecs` | `i64` | 会话时长（秒） |
| `hourOfDay` | `u8` | 完成时的小时数 |
| `finalDifficulty` | `f64` | 最终难度系数 |

---

### 选择下一个单词

**POST** `/api/learning/pick-next-word`

从当前活跃单词列表中选择下一个展示的单词。错误词优先（按最久未展示排序），无错误词时按优先级降序、最久未展示排序。

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `activeWordIds` | `string[]` | 是 | 当前活跃单词 ID 列表（不能为空） |
| `errorWordIds` | `string[]` | 是 | 当前错误单词 ID 列表 |
| `lastShownMap` | `Map<string, u64>` | 否 | 各单词最后展示时间戳 |
| `priorityMap` | `Map<string, u32>` | 否 | 各单词优先级 |

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `word` | `WordPublic` | 选中的单词 |
| `priority` | `string` | `"error_review"` 或 `"normal"` |

---

### 生成选项

**POST** `/api/learning/generate-options`

为指定单词生成四选一选项（含正确答案和 3 个干扰项）。支持"看词选义"和"看义选词"两种模式。

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `wordId` | `string` | 是 | 目标单词 ID |
| `mode` | `string` | 是 | `"word-to-meaning"` 或 `"meaning-to-word"` |
| `poolWordIds` | `string[]` | 是 | 干扰项候选单词 ID 池 |

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `options` | `string[]` | 四个选项（已随机排列） |
| `correctIndex` | `usize` | 正确答案的索引 |

---

## 学习配置（/api/study-config）

### 获取学习配置

**GET** `/api/study-config`

获取当前用户的学习配置，未设置时返回默认值。

**请求参数**：无

**响应体**：`UserStudyConfig` 对象。

| 字段 | 类型 | 说明 |
|------|------|------|
| `userId` | `string` | 用户 ID |
| `selectedWordbookIds` | `string[]` | 已选词书 ID 列表 |
| `dailyWordCount` | `u32` | 每日单词数量 |
| `studyMode` | `string` | `"normal"` / `"intensive"` / `"review"` / `"casual"` |
| `dailyMasteryTarget` | `u32` | 每日掌握目标 |

---

### 更新学习配置

**PUT** `/api/study-config`

更新当前用户的学习配置，所有字段均为可选（部分更新）。

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `selectedWordbookIds` | `string[]` | 否 | 已选词书 ID（会验证是否存在） |
| `dailyWordCount` | `u32` | 否 | 每日单词数量（限制 1~200） |
| `studyMode` | `string` | 否 | 学习模式 |
| `dailyMasteryTarget` | `u32` | 否 | 每日掌握目标（限制 1~100） |

**响应体**：更新后的 `UserStudyConfig` 对象（结构同上）。

---

### 获取今日单词

**GET** `/api/study-config/today-words`

获取今日待学习的单词列表。自动排除今日已学过的单词，按 `dailyWordCount` 限制数量。

**请求参数**：无

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `words` | `Word[]` | 今日单词列表 |
| `target` | `u32` | 每日目标数量 |

---

### 获取学习进度

**GET** `/api/study-config/progress`

获取当前用户的单词学习状态统计。

**请求参数**：无

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `studied` | `u64` | 已学习数（mastered + reviewing） |
| `target` | `u32` | 每日掌握目标 |
| `new` | `u64` | 新词数量 |
| `learning` | `u64` | 学习中数量 |
| `reviewing` | `u64` | 复习中数量 |
| `mastered` | `u64` | 已掌握数量 |

---

## 公共类型参考

### WordPublic

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | `string` | 单词 ID |
| `text` | `string` | 单词文本 |
| `meaning` | `string` | 释义 |
| `pronunciation` | `string?` | 发音 |
| `partOfSpeech` | `string?` | 词性 |
| `difficulty` | `f64` | 难度系数 |
| `examples` | `string[]` | 例句 |
| `tags` | `string[]` | 标签 |
| `createdAt` | `DateTime` | 创建时间 |
