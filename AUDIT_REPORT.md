# 项目审计报告

> 审计日期：2026-02-12
> 审计范围：全栈（SolidJS 前端 + Rust Axum 后端）
> 审计类型：功能性、占位符、硬编码

---

## 一、占位符 / Stub 实现（功能缺失）

| 严重度 | 文件 | 行号 | 描述 |
|--------|------|------|------|
| HIGH | `src/workers/monitoring_aggregate.rs` | 3-6 | 完全 stub，仅打日志 `"done (stub)"`，已禁用 |
| HIGH | `src/workers/llm_advisor.rs` | 3-6 | 完全 stub，仅打日志 `"done (stub)"` |
| HIGH | `src/workers/embedding_generation.rs` | 18-21 | 获取无 embedding 的单词后什么都不做，日志 `"embedding service integration pending"`，已禁用 |
| HIGH | `src/workers/etymology_generation.rs` | 47 | 生成占位文本 `"Auto-generated etymology for '{}'"` 而非真实词源，已禁用 |
| HIGH | `src/routes/content.rs` | 67-74 | `get_etymology` 返回占位数据 `status: "pending_llm"`，LLM 集成未完成 |
| MEDIUM | `src/routes/content.rs` | 112-121 | `semantic_search` 注释 `"Fallback to text search until embeddings are available"`，embedding 未实现 |
| MEDIUM | `src/amas/memory/evm.rs` | 全文件 | 在 `mod.rs` 中声明但从未被 `engine.rs` 调用，属于死代码（B39 Encoding Variability Model） |

---

## 二、安全相关硬编码

| 严重度 | 文件 | 行号 | 描述 |
|--------|------|------|------|
| LOW | `src/config.rs` | 168-169 | JWT_SECRET 默认值 `"change_me_to_random_64_chars..."`（有 `validate_secrets()` 保护，非生产风险） |
| LOW | `src/config.rs` | 200-203 | ADMIN_JWT_SECRET 同上 |
| LOW | `src/config.rs` | 316-317 | 默认密钥字符串在验证函数中重复硬编码，与 168/201 行存在失同步风险 |
| LOW | `src/routes/auth.rs` | 340-341 | `tracing::trace!` 输出原始 reset token，日志级别配置不当可能泄露 |
| INFO | `src/main.rs` | 87-88 | CSP 安全头硬编码在代码中，不可配置 |
| INFO | `src/main.rs` | 93 | HSTS `max-age=31536000`（1年）硬编码 |

---

## 三、授权 / 验证缺陷

| 严重度 | 文件 | 行号 | 描述 |
|--------|------|------|------|
| MEDIUM | `src/routes/word_states.rs` | 90-116 | `mark_mastered` 不验证 word 是否存在于系统中 |
| MEDIUM | `src/routes/word_states.rs` | 118-137 | `reset_word` 不验证 word 是否存在 |
| MEDIUM | `src/routes/word_states.rs` | 153-196 | `batch_update` 不验证 word_id 是否存在 |
| MEDIUM | `src/routes/admin/mod.rs` | 99-117 | `ban_user` / `unban_user` 不检查目标用户是否存在 |
| LOW | `src/routes/content.rs` | 263 | `get_confusion_pairs` 全表遍历，数据量大时有性能风险 |
| INFO | `src/routes/health.rs` | 64-65 | `errorCount` 硬编码返回 `0`，不反映真实错误计数 |
| INFO | `src/routes/health.rs` | 30-38 | 公开 `/health` 端点暴露 `uptimeSecs` |

---

## 四、硬编码魔法数字 — 后端核心

### 4.1 Store 操作层

| 文件 | 行号 | 值 | 描述 |
|------|------|-----|------|
| `src/store/operations/users.rs` | 8 | `5` | `MAX_FAILED_LOGIN_ATTEMPTS`（与 admins.rs 重复） |
| `src/store/operations/users.rs` | 10 | `15` | `LOCKOUT_DURATION_MINUTES`（与 admins.rs 重复） |
| `src/store/operations/admins.rs` | 8, 10 | `5`, `15` | 同上，重复定义 |
| `src/store/operations/learning_sessions.rs` | 44 | `20` | `MAX_CAS_RETRIES`（3 处独立定义） |
| `src/store/operations/users.rs` | 31 | `20` | `MAX_CAS_RETRIES`（重复） |
| `src/store/operations/admins.rs` | 12 | `20` | `MAX_CAS_RETRIES`（重复） |
| `src/store/operations/system_settings.rs` | 17-21 | `10000, 20` | `max_users`, `default_daily_words` 等默认值 |
| `src/store/operations/study_configs.rs` | 28-33 | `20, 10` | `daily_word_count`, `daily_mastery_target` 默认值 |
| `src/store/operations/sessions.rs` | 202 | `1000` | `MAX_BATCH_SIZE` 清理批次大小 |

### 4.2 路由层

| 文件 | 行号 | 值 | 描述 |
|------|------|-----|------|
| `src/routes/auth.rs` | 95 | `10` | `MAX_SESSIONS_PER_USER` |
| `src/routes/words.rs` | 69 | `20/100` | 默认/最大分页（与其他路由使用 config 不一致） |
| `src/routes/v1.rs` | 72 | `50` | 默认 `per_page` |
| `src/routes/v1.rs` | 97 | `5000` | `DEDUP_WINDOW_MS` |
| `src/routes/records.rs` | 38-39 | `50/100` | 默认/最大分页 |
| `src/routes/content.rs` | 264 | `100` | confusion pairs 上限 |
| `src/routes/user_profile.rs` | 194 | `[9,14,20]` | 默认学习时段 |
| `src/routes/user_profile.rs` | 231 | `512*1024` | `MAX_AVATAR_SIZE` |
| `src/routes/words.rs` | 284 | `10MB` | `MAX_RESPONSE_SIZE` |
| `src/routes/word_states.rs` | 104, 129 | `24.0` | `half_life` 默认值（两处） |
| `src/routes/notifications.rs` | 90-117 | — | 3 个 badge 定义硬编码（含英文字符串） |
| `src/routes/notifications.rs` | 155-162 | `"light"`, `"en"` | 默认主题和语言 |

### 4.3 Worker 层

| 文件 | 行号 | 值 | 描述 |
|------|------|-----|------|
| `src/workers/mod.rs` | 31 | `300s` | `WORKER_TIMEOUT` 不可配置 |
| `src/workers/mod.rs` | 36-37 | `30s` | `DRAIN_TIMEOUT` 不可配置 |
| `src/workers/mod.rs` | 120-213 | 17 条 | 所有 cron 调度表达式硬编码 |
| `src/workers/cache_cleanup.rs` | 7, 13 | `10000`, `7天` | 批次上限、过期天数 |
| `src/workers/weekly_report.rs` | 6, 36 | `500`, `10000` | 用户批次、每用户记录上限 |
| `src/workers/confusion_pair_cache.rs` | 6, 31, 59 | `100`, `500`, `10` | 批次、记录数、最大配对数 |
| `src/workers/word_clustering.rs` | 6, 36-42 | `5000`, `0.33/0.66` | 页大小、难度阈值 |
| `src/workers/health_analysis.rs` | 6, 35, 53 | `100`, `100`, `0.3` | 批次、记录数、风险阈值 |
| `src/workers/forgetting_alert.rs` | 12, 80 | `48h`, `3600000` | 去重窗口、毫秒魔法数 |

---

## 五、硬编码魔法数字 — AMAS 引擎（~30 个）

| 文件 | 行号 | 值 | 描述 |
|------|------|-----|------|
| `src/amas/memory/mastery.rs` | 39 | `0.3, 0.1, 0.5` | alpha 计算参数 |
| `src/amas/memory/mastery.rs` | 84 | `0.2` | 遗忘阈值 |
| `src/amas/memory/mdm.rs` | 74-88 | `0.9, 0.02, 0.6, 0.05, 0.2, 0.03` | 自适应保留率阈值和调整量 |
| `src/amas/memory/mdm.rs` | 88 | `0.70, 0.95` | 保留率 clamp 范围 |
| `src/amas/memory/mdm.rs` | 119 | `365天, 60秒` | 最大/最小间隔 |
| `src/amas/decision/heuristic.rs` | 13-15 | `0.4, 5, 0.1` | 疲劳上限 |
| `src/amas/decision/heuristic.rs` | 28, 33 | `0.1, 0.2` | 难度下限 |
| `src/amas/decision/swd.rs` | 66-67 | `7天` | 时间衰减半衰期 |
| `src/amas/decision/swd.rs` | 98 | `0.2, 0.9` | 置信度 clamp 范围 |
| `src/amas/decision/swd.rs` | 131 | `1000000` | 归一化参考值 |
| `src/amas/decision/ige.rs` | 120 | `1e6` | 未探索 bin 分数 |
| `src/amas/memory/evm.rs` | 18, 28 | `5.0, 0.3, -0.2` | 除数、奖励上限、衰减率 |
| `src/amas/engine.rs` | 69 | `500` | 用户锁清理阈值 |
| `src/amas/engine.rs` | 485, 494 | `0.5` | 动机/信心信号二值阈值（无渐变） |
| `src/amas/engine.rs` | 515-520 | `0.5` | 趋势基线（3 处） |
| `src/amas/types.rs` | 89-93 | `0.7, 0.1` | `UserState` 默认 attention/confidence |
| `src/amas/types.rs` | 158 | `[9,14,20], 15.0, 1.0` | `HabitProfile` 默认值 |
| `src/amas/metrics_persistence.rs` | 44 | 字符串数组 | 算法 ID 硬编码，与枚举存在漂移风险 |

---

## 六、硬编码魔法数字 — 前端

### 6.1 Stores / Libs

| 文件 | 行号 | 值 | 描述 |
|------|------|-----|------|
| `frontend/src/stores/ui.ts` | 36 | `6000` | 错误 toast 时长，未用 `constants.ts` |
| `frontend/src/lib/token.ts` | 86, 116 | `300` | Token 刷新缓冲 5 分钟（两处重复） |
| `frontend/src/lib/queryClient.ts` | 6-7 | `120000, 600000` | staleTime / gcTime |
| `frontend/src/lib/WordQueueManager.ts` | 139 | `1000` | 答题历史上限 |
| `frontend/src/lib/WordQueueManager.ts` | 222 | `5` | 近期窗口大小 |
| `frontend/src/workers/fatigue.worker.ts` | 98-103 | `0.3, 0.2` | 表情疲劳权重 |
| `frontend/src/hooks/useFatigueDetection.ts` | 14, 16 | `100, 5000` | 捕获/上报间隔 |

### 6.2 Pages

| 文件 | 行号 | 值 | 描述 |
|------|------|-----|------|
| `pages/LoginPage.tsx` | 28-29 | `3, 30000` | 冷却阈值和上限 |
| `pages/LearningPage.tsx` | 254 | `1500` | 完成延迟 |
| `pages/LearningPage.tsx` | 275 | `1000/2000` | 正确/错误自动前进延迟 |
| `pages/LearningPage.tsx` | 337 | `[10,15,20,30]` | 目标预设 |
| `pages/LearningPage.tsx` | 377 | `100` | 自定义目标上限 |
| `pages/FlashcardPage.tsx` | 86 | `300000` | 疲劳警告冷却 5 分钟 |
| `pages/StatisticsPage.tsx` | 144 | `14` | 显示最近天数 |
| `pages/StatisticsPage.tsx` | 148 | `0.8, 0.5` | 准确率颜色阈值 |
| `pages/VocabularyPage.tsx` | 258 | `50` | 批量导入块大小 |
| `pages/AdminLoginPage.tsx` | 70 | `30` | 最大锁定秒数 |
| `pages/SettingsPage.tsx` | 38-43 | `100000, 500` | 最大用户数、每日单词上限 |
| `pages/MonitoringPage.tsx` | 44 | `20` | 监控事件获取上限 |

### 6.3 API 层

| 文件 | 行号 | 值 | 描述 |
|------|------|-----|------|
| `frontend/src/api/client.ts` | 9-11 | `30000, 3000, 30000` | 请求超时、SSE 重连初始/最大延迟 |
| `frontend/src/api/wordStates.ts` | 13 | `50` | getDueList 默认 limit |
| `frontend/src/api/content.ts` | 8 | `10` | semanticSearch 默认 limit |
| `frontend/src/api/amas.ts` | 60 | `50` | getMonitoring 默认 limit |

---

## 七、代码重复 / 维护风险

| 文件 | 描述 |
|------|------|
| `src/store/operations/users.rs` + `admins.rs` | `MAX_FAILED_LOGIN_ATTEMPTS`、`LOCKOUT_DURATION_MINUTES` 重复定义 |
| `src/store/operations/users.rs` + `admins.rs` + `learning_sessions.rs` | `MAX_CAS_RETRIES = 20` 独立定义 3 次 |
| `src/store/operations/users.rs:42,262` | 直接硬编码 `"email:"` 前缀，绕过 `keys.rs` |
| `src/store/operations/word_states.rs:54-65` | `parse_due_index_item_key` 与 `keys.rs:256-267` 重复实现 |
| `frontend/src/stores/theme.ts:7,9,35` | 使用自有 key `'eng_theme'` 绕过 `storage.ts` 封装，`STORAGE_KEYS.THEME` 定义但未使用 |
| `frontend/src/pages/FlashcardPage.tsx:84-91` + `LearningPage.tsx:261-267` | 疲劳警告冷却逻辑完全重复 |
| `frontend/src/lib/storage.ts:17` | `SESSION_BACKED_KEYS` 集合为空，基础设施存在但未使用 |

---

## 八、类型可收窄（前端）

| 文件 | 行号 | 字段 | 建议 |
|------|------|------|------|
| `types/user.ts` | 39 | `UserPreferences.theme` | `string` → `'light' \| 'dark'` |
| `types/user.ts` | 40 | `UserPreferences.language` | `string` → `'zh-CN' \| 'en'` |
| `types/admin.ts` | 47 | `SystemHealth.status` | `string` → `'healthy' \| 'degraded' \| 'down'` |
| `types/admin.ts` | 49 | `SystemHealth.uptime` | `string \| number` → 统一为一种类型 |
| `types/amas.ts` | 100 | `MasteryEvaluation.state` | `string` → 复用 `WordStateType` |
| `types/amas.ts` | 184 | `MonitoringEvent.data` | `Record<string, unknown>` → 按事件类型定义联合类型 |
| `types/content.ts` | 13 | `Morpheme.type` | `string` → `'prefix' \| 'root' \| 'suffix'` |
| `types/learning.ts` | 67 | `AdjustWordsRequest.userState` | `string` → 定义有效值联合类型 |
| `types/userProfile.ts` | 24 | `LearningStyle.style` | `string` → `'visual' \| 'auditory' \| 'reading' \| 'kinesthetic'` |
| `types/admin.ts` | 15 | `AdminUsersQuery` | `[key: string]` 索引签名允许任意键，削弱类型安全 |

---

## 九、组件硬编码颜色（不跟随主题）

| 文件 | 行号 | 描述 |
|------|------|------|
| `components/fatigue/FatigueIndicator.tsx` | 7-10 | 4 个硬编码 hex 颜色 `#22c55e, #eab308, #f97316, #ef4444` + 原始 Tailwind 颜色类 |
| `components/ui/Select.tsx` | 30 | SVG data URI 中硬编码 `%236b7280`（灰色），不随主题变化 |
| `components/ui/Button.tsx` | 9-11 | danger/success/warning 变体使用 `text-white` 而非语义化 token |
| `components/ui/Switch.tsx` | 27 | 切换旋钮使用 `bg-white` 而非语义化 token |
| `components/ui/Modal.tsx` | 95 | 遮罩层使用 `bg-black/50` 而非语义化 overlay token |

---

## 十、Store 层性能 TODO（注释中，非运行时 panic）

| 文件 | 行号 | 描述 |
|------|------|------|
| `store/operations/wordbooks.rs` | 55-56, 71 | 需要类型前缀索引，当前全表扫描 |
| `store/operations/users.rs` | 38, 257 | 需要原子计数器 / 时间戳索引 |
| `store/operations/records.rs` | 27, 41 | 需要全局时间戳索引 |
| `store/operations/words.rs` | 62, 78, 96, 116, 199 | 需要文本索引、反向索引、全文搜索索引 |

---

## 统计摘要

| 类别 | 数量 |
|------|------|
| Stub / 占位符实现 | 4 个 worker + 2 个路由 + 1 个死代码模块 |
| 安全相关硬编码 | 6 处（均有缓解措施） |
| 授权 / 验证缺陷 | 7 处 |
| AMAS 引擎魔法数字 | ~30 个 |
| 后端其他魔法数字 | ~35 个 |
| 前端魔法数字 | ~25 个 |
| 代码重复 / 维护风险 | 7 处 |
| 类型可收窄 | 10 处 |
| 组件硬编码颜色 | 5 处 |
| Store 性能 TODO | 11 处 |
| `todo!()` / `unimplemented!()` 运行时 panic | 0 |
