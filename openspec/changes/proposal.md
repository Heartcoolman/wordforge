# OpenSpec Proposal: WordForge 全量代码审计

**日期**: 2026-04-15
**范围**: 后端 (Rust/Axum) + 前端 (SolidJS/TypeScript) 全量审查
**目标**: 定位 bug、功能失效、硬编码问题

---

## 一、审计方法

按上下文边界将代码库划分为 3 个独立审查域，并行深度审查：

| 审查域 | 覆盖范围 |
|--------|----------|
| Routes/Auth/Middleware | `src/routes/**`, `src/auth.rs`, `src/middleware/**`, `src/response.rs`, `src/extractors.rs`, `src/validation.rs` |
| AMAS Engine/Workers/Store | `src/amas/**`, `src/workers/**`, `src/store/**`, `src/services/**`, `src/state.rs`, `src/config.rs` |
| Frontend | `frontend/src/**` |

---

## 二、发现汇总

### 严重级别统计

| 级别 | Bug | 功能失效 | 硬编码 | 合计 |
|------|-----|----------|--------|------|
| **Critical** | 3 | 1 | 0 | **4** |
| **High** | 8 | 7 | 0 | **15** |
| **Medium** | 24 | 4 | 8 | **36** |
| **Low** | 17 | 5 | 40+ | **62+** |

---

## 三、Critical 严重度问题（必须立即修复）

### C1. SSE 解析器破坏多行事件 + 丢弃默认 message 类型
- **文件**: `frontend/src/api/client.ts` L289-315
- **类型**: Bug
- **描述**: 两个严重问题：(1) SSE 解析器在处理完第一个 `data:` 行后立即重置 `eventType`（L311），破坏 SSE 规范中多行 data 事件——后续 data 行因 `eventType` 为空被 `else if (line.startsWith('data:') && eventType)` 守卫静默丢弃。(2) 无显式 `event:` 字段的 SSE 事件（SSE 规范默认为 `message` 类型）因 `eventType` 初始为空而全部被丢弃。
- **证据**:
  ```typescript
  if (line.startsWith('event:')) {
    eventType = line.slice(6).trim();
  } else if (line.startsWith('data:') && eventType) {  // ← 无event字段的被丢弃
    try { const data = JSON.parse(line.slice(5).trim()); /* dispatch... */ }
    catch { }
    eventType = '';   // ← 每行后重置，破坏多行data
  } else if (line === '') {
    eventType = '';
  }
  ```

### C2. localStorage 访问无 try-catch，受限环境全应用崩溃
- **文件**: `frontend/src/lib/device.ts` L12-16
- **类型**: Bug
- **描述**: `getDeviceId()` 直接调用 `localStorage.getItem/setItem` 无异常保护。Safari 隐私模式、iframe 等受限环境中 localStorage 不可用会抛出异常。由于 `getDeviceId()` 在每个 API 请求中被调用（通过 `X-Device-Id` header），此问题会导致所有网络请求失败。项目自身的 `storage.ts` 已正确包裹 try-catch，此处遗漏。
- **证据**:
  ```typescript
  export function getDeviceId(): string {
    let id = localStorage.getItem(DEVICE_ID_KEY);  // ← 无try-catch
    if (!id) {
      id = generateUUID();
      localStorage.setItem(DEVICE_ID_KEY, id);     // ← 无try-catch
    }
    return id;
  }
  ```

### C3. engine_monitoring_events 的 reward_json 列写入完整事件 JSON
- **文件**: `src/store/operations/engine.rs` L112-114
- **类型**: Bug
- **描述**: INSERT 语句对 `strategy_json` 和 `reward_json` 列使用相同的参数 `?5`：`VALUES (?1, ?2, ?3, ?4, ?5, ?5)`。`reward_json` 列存储的是完整事件 JSON 而非实际 reward 数据，所有读取 `reward_json` 的监控查询都会返回整个事件对象。
- **证据**:
  ```rust
  "INSERT INTO engine_monitoring_events (id, user_id, session_id, timestamp, strategy_json, reward_json) VALUES (?1, ?2, ?3, ?4, ?5, ?5)"
  // 最后一个 ?5 应为 ?6（独立的 reward 参数）
  ```

### C4. reset_user_state 仅清理 3 种 algo key，遗漏 MTP/IAD/Mastery 逐词状态
- **文件**: `src/amas/engine.rs` L298-315
- **类型**: 功能失效
- **描述**: `reset_user_state` 仅删除 `ige`、`swd`、`trust` 三种算法状态，遗漏：(1) 逐词 mastery 状态 `mastery:{word_id}`，(2) IAD 状态 `iad`，(3) MTP 状态。重置后这些陈旧状态在下次 `process_event` 时被加载，导致重置对实际算法状态无效。
- **证据**:
  ```rust
  for algo in &["ige", "swd", "trust"] {
      self.store.delete_engine_algo_state(user_id, algo)...
  }
  // 缺少: "iad", "mtp", "mastery:*" 的清理
  ```

---

## 四、High 严重度问题（必须修复）

### H1. 用户登录密码预言机漏洞
- **文件**: `src/routes/auth.rs` L226-244
- **类型**: Bug (安全)
- **描述**: 用户登录流程在密码验证**之后**才检查账户锁定/封禁状态。攻击者可通过不同错误响应（锁定账户+错误密码→401 vs 锁定账户+正确密码→429）判断密码正确性。同时存在时序侧信道：已有用户输错密码执行 `record_failed_login` DB 写入，不存在用户无 DB 写入，可枚举邮箱。
- **对比**: `src/routes/admin/auth.rs` L141-147 已正确在密码验证**之前**检查锁定。
- **修复**: 将 `is_banned`、`is_account_locked` 移到 `verify_password` 之前；用户不存在时也执行假 DB 写入消除时序差。

### H2. 设备封禁检查 DB 错误时放行请求
- **文件**: `src/middleware/device.rs` L25-39
- **类型**: Bug (安全)
- **描述**: `is_device_banned` 返回错误时请求被放行。攻击者可利用 DB 不可用绕过设备封禁。`upsert_client_device` 错误同样被静默吞掉。
- **证据**:
  ```rust
  match state.store().is_device_banned(did) {
      Ok(true) => { return FORBIDDEN }
      Ok(false) => {}
      Err(e) => { tracing::error!(...); /* 请求继续！ */ }
  }
  ```

### H3. Admin 用户搜索加载全部用户到内存
- **文件**: `src/routes/admin/mod.rs` L108-125
- **类型**: 功能失效
- **描述**: 使用 `list_users(usize::MAX, 0)` 加载全部用户到内存后 Rust 侧过滤。10,000 用户时内存暴涨、响应延迟。搜索/过滤应下推到数据层。

### H4. ssp_policy 构建后永不随 config reload 更新
- **文件**: `src/amas/engine.rs` L47-56,67-76
- **类型**: 功能失效
- **描述**: `AMASEngine::new` 从 config 预计算 SSP policy 表。`reload_config` 更新 `self.config` 但从未重算 `self.ssp_policy`。SSP policy 冻结在启动值，config 变更不生效。
- **证据**: L47-56: `let ssp_policy = if config.feature_flags.ssp_enabled { ... }` — 计算一次。L67-76: `reload_config` 只更新 `self.config`。

### H5. schema.rs 与 migrate.rs 中 client_devices/telemetry_events 表定义重复
- **文件**: `src/store/schema.rs` L419-446, `src/store/migrate.rs` L68-96
- **类型**: Bug
- **描述**: 相同的 CREATE TABLE 语句出现在 schema.rs DDL 和 migrate.rs m002 中。虽 IF NOT EXISTS 防止报错，但重复定义有分歧风险：在一处添加列而另一处遗漏时，新建库和迁移库 schema 不同。

### H6. forgetting_alert worker N+1 查询 + 去重写入错误静默丢弃
- **文件**: `src/workers/forgetting_alert.rs` L1-95
- **类型**: 功能失效
- **描述**: (1) N+1+N 查询模式：遍历用户→每用户查到期词→每词查去重+插通知，随用户数增长严重降级。(2) `let _ = store.set_alert_dedup(...)` 静默丢弃错误，去重写入失败导致下次运行重复通知。

### H7. health_analysis MAX_RECORDS_PER_USER=100 不足 7 天窗口 + 准确率偏差
- **文件**: `src/workers/health_analysis.rs` L37-62
- **类型**: Bug
- **描述**: 每用户最多加载 100 条记录（按 created_at DESC），但需分析 7 天窗口。7 天内超过 100 条记录时只加载最新 100 条，导致计数不足。准确率计算基于截断样本，产生偏差。

### H8. weekly_report 加载 10,000 条记录但可能全在窗口外
- **文件**: `src/workers/weekly_report.rs` L38-42
- **类型**: Bug
- **描述**: `get_user_records(&user.id, MAX_RECORDS_PER_USER)` 加载 10k 记录。若用户有 10k+ 更早记录，本周记录可能一条都不在加载范围内，导致活跃用户被误判为不活跃。

### H9. heartbeat_watchdog 未注册到 WorkerName 枚举/调度器
- **文件**: `src/workers/heartbeat_watchdog.rs` L1-65
- **类型**: 功能失效
- **描述**: heartbeat_watchdog 模块存在但未包含在 `WorkerName` 枚举中，从未被调度运行。此外，发送告警后 miss count 重置为 0，允许同一告警在下一扫描周期（5 秒后）再次触发，产生通知风暴。

### H10. ensemble update_trust 忽略 Mdm/Mastery/Ensemble 算法 ID
- **文件**: `src/amas/decision/ensemble.rs` L142-147
- **类型**: 功能失效
- **描述**: `update_trust` 仅匹配 Heuristic/Ige/Swd，其他算法 ID 直接 return。AMAS 引擎使用 Mdm 和 Mastery 算法 ID 进行逐词 mastery 追踪，调用 `update_trust` 时这些贡献的信任分数从不更新。集成系统无法学习信任这些算法。

### H11. set_engine_user_state 不更新反规范化列
- **文件**: `src/store/operations/engine.rs` L26-42
- **类型**: 功能失效
- **描述**: `engine_user_states` 表有 attention、fatigue、motivation 等反规范化列，但 `set_engine_user_state` 仅更新 `state_json`。反规范化列在首次 insert 后永远过时，直接查询这些列的代码得到过时值。

### H12. create_notification 允许空 user_id 和 id
- **文件**: `src/store/operations/extras.rs` L569-586
- **类型**: Bug
- **描述**: 从 JSON 提取 user_id/id 使用 `unwrap_or_default()`，缺失字段时产生空字符串。notifications 表 PRIMARY KEY 为 (user_id, id)，空值可导致约束冲突或插入无效记录。

### H13. sessionStorage 访问无 try-catch
- **文件**: `frontend/src/lib/fatigueWarningCooldown.ts` L5,11
- **类型**: Bug
- **描述**: 模块顶层和函数内直接调用 `sessionStorage.getItem/setItem` 无 try-catch。Safari 隐私模式、受限 iframe 环境中 sessionStorage 不可用导致模块加载崩溃。

### H14. Switch 组件缺少 HTML disabled 属性
- **文件**: `frontend/src/components/ui/Switch.tsx` L16-19
- **类型**: Bug
- **描述**: Switch 组件 disabled 状态仅设置 CSS opacity/pointer-events，未设置 `<button>` 的 `disabled` HTML 属性。按钮仍可通过 Tab 聚焦、Space/Enter 激活，违反可访问性要求。

### H15. AMAS MemoryModelConfig Default/Serde 默认值不一致
- **文件**: `src/amas/config.rs`
- **类型**: Bug
- **描述**: `MemoryModelConfig` 的 `Default` trait 与 serde `#[serde(default = "...")]` 函数返回不同值，导致不同构造路径产生不同算法行为。
- **不一致字段**:
  - `base_desired_retention`: serde=0.85 vs Default=0.92
  - `passive_decay_power`: serde=0.5 vs Default=0.30
  - `forgetting_curve_factor`: serde≈0.2346 vs Default=0.30
  - `forgetting_curve_floor`: serde=0.10 vs Default=0.0

---

## 五、Medium 严重度问题（应当修复）

### M1. daily_aggregation 聚合时间错误
- **文件**: `src/workers/daily_aggregation.rs` L10-13
- **类型**: Bug
- **描述**: Cron 设为 UTC 1:00 AM 运行，但聚合"今天"数据，仅 1 小时数据。应改为聚合"昨天"。

### M2. 维护模式 /status 子路径未豁免
- **文件**: `src/middleware/maintenance.rs` L14-18
- **类型**: Bug
- **描述**: `path == "/status"` 仅精确匹配，`/status/device-ban` 返回 503。应改为 `path.starts_with("/status")`。

### M3. 设备封禁/维护模式中间件响应格式不一致
- **文件**: `src/middleware/device.rs` L27-34, `src/middleware/maintenance.rs` L21-28
- **类型**: Bug
- **描述**: 直接返回原始 JSON，缺少 `success: false` 和 `traceId` 字段。

### M4. 广播标题/消息长度校验使用字节数
- **文件**: `src/routes/admin/broadcast.rs` L27-39
- **类型**: Bug
- **描述**: `.len()` 返回字节数，中文占 3 字节，实际允许约 66 个中文字符而非 200 个。应用 `.chars().count()`。

### M5. 学习会话可重复完成
- **文件**: `src/routes/learning.rs` L489-539
- **类型**: Bug
- **描述**: `complete_session` 未校验会话状态，已完成会话可被重复调用。

### M6. Cookie 缺少 Max-Age 属性
- **文件**: `src/routes/auth.rs` L429-437
- **类型**: Bug
- **描述**: refresh token cookie 为会话级，浏览器关闭即删除，与其长期刷新设计矛盾。Cookie 属性硬编码 SameSite=None，同站点部署时应为 Lax。

### M7. weekly_report 静默吞错
- **文件**: `src/workers/weekly_report.rs` L24-26
- **类型**: Bug
- **描述**: `list_users` 失败时 `unwrap_or_default()` 静默返回空 Vec，生成 `total_users=0` 虚假报告。

### M8. .env.example 遗留配置
- **文件**: `.env.example` L7
- **类型**: 功能失效
- **描述**: `SLED_PATH=./data/learning.sled` 为旧版遗留，实际使用 `DATABASE_URL`（SQLite），缺少该条目。

### M9. AMAS is_new_context 逻辑错误
- **文件**: `src/amas/engine.rs` L870-874
- **类型**: Bug
- **描述**: `is_new_context` 在 session_id 非空时即设为 `true`，应仅在 session_id 实际**变更**时触发。

### M10. GitHub URL 硬编码
- **文件**: `src/routes/admin/monitoring.rs` L105
- **类型**: 硬编码
- **描述**: 更新检查 URL 硬编码，fork 部署需修改源码。未认证 API 访问 60次/小时限额。

### M11. 前端 MaintenanceProvider 路由检测非响应式
- **文件**: `frontend/src/App.tsx` L62,76
- **类型**: Bug
- **描述**: `isAdminPath()` 直接读 `window.location.pathname`，SPA 导航时不触发重计算。

### M12. SSE 未认证时无限重连
- **文件**: `frontend/src/api/client.ts` L317-321
- **类型**: 功能失效
- **描述**: 无 token 时 SSE 流每 30 秒重试一次，永不停止。缺少暂停/恢复机制。

### M13. Rate-Limit Retry-After 头设为窗口总时长
- **文件**: `src/middleware/rate_limit.rs` L209-211
- **类型**: Bug
- **描述**: RFC 7231 要求应为剩余等待时间 `reset_after`，实际设为 `window_secs`。

### M14. Rate-Limit 无 IP 时降级到 127.0.0.1 共享桶
- **文件**: `src/middleware/rate_limit.rs` L260
- **类型**: Bug
- **描述**: 所有用户共享单一限流桶，少数活跃用户可耗尽全局限额。

### M15. StoreError 错误映射绕过 400 处理
- **文件**: `src/routes/notifications.rs` L99-107,114,175
- **类型**: Bug
- **描述**: `.map_err(|e| AppError::internal(...))` 绕过 `From<StoreError>` 实现，`StoreError::Validation` 返回 500 而非 400。

### M16. 用户登录时序泄漏：可枚举邮箱
- **文件**: `src/routes/auth.rs` L218-244
- **类型**: Bug (安全)
- **描述**: 已有用户输错密码执行 DB 写入，不存在用户无 DB 写入，时序差可枚举邮箱。

### M17. refresh token 提取回退到 access token（死代码+潜在漏洞）
- **文件**: `src/auth.rs` L152-166
- **类型**: Bug
- **描述**: 回退路径因 JWT secret 不同永远失败（死代码）。若两 secret 误配相同值，则变成安全漏洞。

### M18. V1 记录去重策略薄弱
- **文件**: `src/routes/v1.rs` L97-108
- **类型**: Bug
- **描述**: 朴素去重（10 条+5 秒窗口），并发下漏判/误判。

### M19. UserManagement 直接重置密码不校验 MIN_PASSWORD_LENGTH
- **文件**: `frontend/src/pages/admin/UserManagementPage.tsx` L84-106
- **类型**: Bug
- **描述**: 管理员直接重置可设置 1 字符密码。

### M20. AmasConfig 前端仅做浅层验证
- **文件**: `frontend/src/pages/admin/AmasConfigPage.tsx` L24-39
- **类型**: Bug
- **描述**: 仅检查非 null 非数组对象后直接 `as AmasConfig`，无效局部配置可破坏 AMAS。

### M21. ssp from_tables_with_bins 忽略 SspConfig，硬编码 base=1.05
- **文件**: `src/amas/memory/ssp.rs` L74-83
- **类型**: Bug
- **描述**: 双网格预计算结果构建 SspPolicy 时忽略 SspConfig，硬编码 `base: 1.05` 和 `min_index: 0`。若 config 使用不同值，stability_to_index 映射将错误。单网格 `from_tables` 正确使用 config 值。

### M22. SWD SIMILARITY_CACHE 全局无界增长
- **文件**: `src/amas/decision/swd.rs` L53-54,81-109
- **类型**: Bug
- **描述**: 静态 `Mutex<HashMap>` 无大小限制和淘汰逻辑，随用户增长内存无限膨胀。TTL 检查仅防复用，不清除旧条目。

### M23. daily_aggregation_stats 3 次独立 SQL 可合并为 1 次
- **文件**: `src/store/operations/extras.rs` L543-566
- **类型**: Bug
- **描述**: 相同 WHERE 条件执行 3 次独立查询（total+correct, unique_users, unique_words），可合并为 `SELECT COUNT(*), SUM(is_correct), COUNT(DISTINCT user_id), COUNT(DISTINCT word_id)`。

### M24. list_all_words / list_all_words_with_tags 无分页
- **文件**: `src/store/operations/extras.rs` L333-344,589-600
- **类型**: Bug
- **描述**: SELECT 无 LIMIT，加载全部词到内存，大量词条时内存暴涨。

### M25. get_word_elos_by_ids / batch_get_mastery_values N+1 查询
- **文件**: `src/store/operations/elo.rs` L49-58,71-85
- **类型**: Bug
- **描述**: 逐 ID 调用 get_word_elo/get_engine_algo_state，应使用 `IN (...)` 批量查询。

### M26. delete_word 使用 conn.execute_batch("BEGIN") 而非 transaction() API
- **文件**: `src/store/operations/words.rs` L129-169
- **类型**: Bug
- **描述**: 手动 BEGIN/ROLLBACK/COMMIT 而非 `conn.transaction()` RAII 模式，panic 时事务状态不一致。

### M27. confusion_pair_cache 无时间窗口/上下文检查
- **文件**: `src/workers/confusion_pair_cache.rs` L42-58
- **类型**: Bug
- **描述**: 任意两次连续错误即视为混淆对，无关时间间隔或主题相关性，产生大量误报。

### M28. delayed_reward 只计数不执行任何奖励/状态变更
- **文件**: `src/workers/delayed_reward.rs` L19-47
- **类型**: 功能失效
- **描述**: 遍历用户到期词仅 `evaluated += 1`，无实际奖励计算、状态更新或通知——功能为未完成的存根。

### M29. etymology_generation 占位文本标记 generated:true 不可区分
- **文件**: `src/workers/etymology_generation.rs` L23-31
- **类型**: 功能失效
- **描述**: 占位文本 `Auto-generated etymology for '{word}'` 与真正 LLM 生成条目同样标记 `generated: true`，下游代码无法区分。

### M30. algorithm_optimization 仅调整一个参数无边界/回退
- **文件**: `src/workers/algorithm_optimization.rs` L37-72
- **类型**: 功能失效
- **描述**: 仅调整 `max_difficulty_when_fatigued`（±0.05/0.03），无累积边界、无回退逻辑、clamp (0.2, 0.9) 几乎无约束力。

### M31. cleanup_expired_sessions LIMIT 1000 一次可能清不完
- **文件**: `src/store/operations/sessions.rs` L153-164
- **类型**: Bug
- **描述**: 长时间未清理时过期会话超过 1000 条，需多次运行才能清完。

### M32. SettingsPage/UserManagementPage 确认对话框无 ARIA
- **文件**: `frontend/src/pages/admin/SettingsPage.tsx` L123-165, `frontend/src/pages/admin/UserManagementPage.tsx` L141-161
- **类型**: Bug
- **描述**: 缺少 `role='alertdialog'`、`aria-modal`、焦点陷阱和焦点恢复。

### M33. Etymology pending_llm 缓存删除后永不重建
- **文件**: `src/routes/content.rs` L42-44
- **类型**: Bug
- **描述**: 删除后不重新标记 `pending_llm`，后续请求也生成回退，状态永久化。

### M34. MediaPipe CDN URLs / wordbook_center_url 硬编码
- **文件**: `frontend/src/lib/constants.ts` L74-81, `src/store/operations/system_settings.rs` L25-26,53-54
- **类型**: 硬编码
- **描述**: CDN URL 和 wordbook_center_url 默认值硬编码，中国大陆等受限环境无法替换。wordbook_center_url 默认值在 Default impl 和 get fallback 中重复。

### M35. AMAS 算法参数大量硬编码
- **文件**: `src/amas/memory/evm.rs:6-8`, `src/amas/memory/mdm.rs:5-14`, `src/amas/memory/mastery.rs:9-12`, `src/amas/decision/heuristic.rs:4-8`, `src/amas/decision/swd.rs:9-14`, `src/amas/engine.rs:15-17`, `src/amas/types.rs:4-8`
- **类型**: 硬编码
- **描述**: AMAS 各模块的阈值、衰减常数、缩放因子等全部为模块级 const，不可通过配置调整。影响范围包括：遗忘曲线参数、记忆衰减速率、疲劳调整系数、上下文多样性权重、mastery 更新参数、SSP 基数、用户状态默认值等。

### M36. broadcast_update 默认消息硬编码中文
- **文件**: `src/state.rs` L173
- **类型**: 硬编码
- **描述**: `'有新版本可用，请刷新页面获取最新内容'` 硬编码，无法国际化。

---

## 六、Low 严重度问题

### Bug 类

| # | 文件 | 行 | 描述 |
|---|------|----|------|
| L1 | `src/routes/learning.rs` | 448-477 | `sync_progress` 未校验会话状态，允许对已完成会话更新进度 |
| L2 | `src/routes/user_profile.rs` | 173-181 | `resolve_avatar_dir` fallback 使用编译时 `CARGO_MANIFEST_DIR`，生产环境无效 |
| L3 | `src/routes/auth.rs` | 229,246 | `record_failed_login`/`reset_login_attempts` 错误用 `let _ =` 丢弃 |
| L4 | `src/routes/words.rs` | 510 | `169.254.0.0/16` 检查与 `is_link_local()` 重复，死代码 |
| L5 | `src/auth.rs` | 116 | JWT 验证错误完全丢弃，不同失败模式不可区分 |
| L6 | `src/auth.rs` | 34-37 | 硬编码 dummy Argon2 hash，参数变更后时序防护失效 |
| L7 | `src/routes/records.rs` | 353 | `AppError::internal` 包含原始 StoreError，可能泄露 DB 内部信息 |
| L8 | `src/validation.rs` | 4-5 | 密码长度校验用 `.len()`（字节数），错误消息说"8个字符"有误导 |
| L9 | `src/routes/auth.rs` | 340-343 | 密码重置 token 前 8 字符记录在 trace 级别日志中 |
| L10 | `src/middleware/request_id.rs` | 77-90 | `inject_trace_id` 反序列化整个 JSON 再序列化，可能改变键序 |
| L11 | `src/store/operations/words.rs` | 62-82 | `upsert_word` 使用 INSERT OR REPLACE，静默覆盖并发更新 |
| L12 | `src/store/operations/word_states.rs` | 113-135 | `set_word_learning_state` INSERT OR REPLACE 无 CAS 保护 |
| L13 | `src/store/operations/admins.rs` | 19 | Admin `updated_at` 的 serde default = `Utc::now`，语义错误 |
| L14 | `src/store/operations/extras.rs` | 88-105 | `get_habit_profile` 不必要的 JSON 序列化-反序列化往返 |
| L15 | `src/store/operations/records.rs` | 191-228 | UserStatsAgg 每次插入记录时全量 JSON 序列化/反序列化 word_ids |
| L16 | `src/store/operations/extras.rs` | 480-500 | `take_password_reset_token` 使用 DELETE ... RETURNING，需 SQLite 3.35+ |
| L17 | `src/store/operations/clients.rs` | 48-65 | `upsert_client_device` 覆盖 user_id，允许设备用户重分配 |
| L18 | `src/amas/decision/ige.rs` | 197-203 | UCB tie-breaking 使用 float bit patterns hash，脆弱且非确定性 |
| L19 | `frontend/src/lib/fatigueWarningCooldown.ts` | 3 | SESSION_KEY 使用裸键名，未遵循 `eng_` 前缀命名约定 |
| L20 | `frontend/src/pages/admin/ClientsPage.tsx` | 218 | 日期解析 `replace(' ', 'T') + 'Z'` 假设特定服务端格式 |
| L21 | `frontend/src/stores/ui.ts` | 27-29 | Toast setTimeout 手动关闭后仍触发 |
| L22 | `frontend/src/pages/admin/SettingsPage.tsx` | 202 | wordbookCenterUrl 空输入设 undefined，与 omit 语义不同 |
| L23 | `frontend/src/types/wordState.ts` | 16-21 | `newCount` 用 Count 后缀，其他字段不用，命名不一致 |
| L24 | `src/store/operations/elo.rs` | 8-21 | EloRating::default() 每次创建 EloConfig::default() 仅读 default_elo |

### 功能失效类

| # | 文件 | 行 | 描述 |
|---|------|----|------|
| L25 | `src/middleware/rate_limit.rs` | 181-186 | `starts_with("/api/")` 检查为死代码 |
| L26 | `src/routes/notifications.rs` | 182-205 | `compute_streak_days` 与 `users.rs` 完全重复，应复用 |
| L27 | `frontend/src/components/ui/EChart.tsx` | 22-35 | MutationObserver 监听 documentElement 任意 class 变化导致不必要重渲染 |
| L28 | `frontend/src/api/client.ts` | 282-283 | SSE reader.cancel() 未 await |
| L29 | `frontend/src/pages/admin/AdminDashboard.tsx` | 15 | 内联类型与 SystemHealth 接口不一致 |

### 硬编码类（高频，仅列关键项）

| 位置 | 值 | 说明 |
|------|----|------|
| `src/routes/word_states.rs:139` | `half_life: 24.0` | 应使用 `DEFAULT_HALF_LIFE_HOURS` 常量 |
| `src/routes/auth.rs:94` | `MAX_SESSIONS_PER_USER: 10` | 未纳入 constants 或 config |
| `src/routes/word_states.rs:78` | `clamp(1, 200)` | due_list 上限硬编码 |
| `src/routes/notifications.rs:39` | `clamp(1, 200)` | 通知列表上限硬编码 |
| `src/routes/admin/analytics.rs:25-27` | `default_days: 7` | 应在 constants.rs 定义 |
| `src/routes/wordbook_center.rs:365` | `100_000` | 本地词书加载上限 |
| `src/routes/wordbook_center.rs:177` | `50MB` | 远程 JSON 大小限制 |
| `src/routes/learning.rs:513,298` | `5000` / `12` | fallback 查询限制 / 小时解析默认值 |
| `src/routes/auth.rs:225,333` | `1h`/`4h` | 密码重置 token 过期时间 |
| `src/main.rs:24-25` | CSP/HSTS | 安全头硬编码 |
| `src/workers/*.rs` | 多处 | batch size、阈值、超时、间隔等 40+ 处内联常量 |
| `src/store/operations/extras.rs` | 多处 | reward_type/habit profile/user preferences 默认值散落 |
| `src/workers/monitoring_aggregate.rs:12,19` | `5min`/`1000` | 聚合窗口和事件限制 |
| `src/workers/cache_cleanup.rs:8` | `7 days` | 监控事件保留期 |
| `frontend/src/lib/constants.ts:74-81` | MediaPipe CDN URLs | CDN URL 硬编码 |
| `frontend/src/workers/telemetry.ts:5` | `5000` | 遥测上报间隔 |
| `frontend/src/App.tsx:62` | `30_000` | 状态轮询间隔 |
| `frontend/src/api/client.ts:10-13` | SSE 参数 | 超时/重连参数未集中管理 |
| `frontend/vite.config.ts:17-19` | `localhost:3000` | 开发代理地址硬编码 |
| `frontend/src/pages/admin/AdminDashboard.tsx:110` | `#6366f1` | EChart 备用颜色硬编码 |
| `frontend/src/lib/device.ts:20-22` | `'web'` | 平台检测始终返回硬编码字符串 |
| `frontend/src/lib/WordQueueManager.ts:177-179` | `(无释义)/(未知)` | UI 占位文本硬编码，无 i18n |
| `frontend/src/pages/admin/UserManagementPage.tsx:39` | `pageSize=20` | 重复 `DEFAULT_PAGE_SIZE` 常量 |
| `src/state.rs:87-88` | `16` | broadcast channel 容量 |
| `src/config.rs:214` | `localhost:5173` | CORS origin 开发默认值 |

---

## 七、约束集合

### 硬约束
1. SQLite 后端：单写多读，所有 worker 同进程 `tokio_cron_scheduler` 调度
2. 无邮件系统：密码重置 token 仅日志记录
3. AMAS 参数分散：config.rs 仅覆盖部分参数，大量算法参数内联在各模块
4. 前端为 SolidJS（非 React）：信号/响应式系统与 React hooks 行为不同
5. `GIT_VERSION` 编译时注入，无 git 环境构建将失败

### 软约束
1. JSON 响应统一使用 `ApiResponse<T>` + `ErrorBody` 格式
2. camelCase 序列化约定
3. 分页使用 `constants.rs` 中定义的 `DEFAULT_PAGE_SIZE` / `MAX_PAGE_SIZE`
4. Worker 签名统一为 `pub async fn run(store/engine)`

### 跨模块依赖
1. `Store` (SQLite) 为所有 worker 和路由的数据层
2. `AMASEngine` 被 routes/amas 和多个 worker 共享
3. `broadcast::channel` 用于 maintenance/update/shutdown 跨模块信号
4. `DashMap` 用于 SSE 连接和 heartbeat 追踪

---

## 八、建议修复优先级

### P0（立即修复）
1. **C1** — SSE 解析器：支持多行 data + 默认 message 类型
2. **C2** — device.ts / fatigueWarningCooldown.ts storage API try-catch 保护
3. **C3** — engine_monitoring_events reward_json 参数修复（?5→?6）
4. **C4** — reset_user_state 补全 MTP/IAD/Mastery 清理
5. **H1** — 登录密码预言机：锁定/封禁检查移到密码验证前，消除时序差
6. **H2** — 设备封禁 DB 错误时拒绝请求（fail-closed）
7. **H4** — ssp_policy 在 reload_config 时重算
8. **H10** — ensemble update_trust 支持 Mdm/Mastery 算法 ID
9. **H11** — set_engine_user_state 同步更新反规范化列
10. **H15** — AMAS 配置默认值统一

### P1（近期修复）
11. **H3** — Admin 用户搜索下推到数据层
12. **H5** — schema.rs 与 migrate.rs 消除重复表定义
13. **H6** — forgetting_alert 批量查询 + 去重写入错误处理
14. **H7/H8** — health_analysis/weekly_report 记录加载修复
15. **H9** — heartbeat_watchdog 注册到调度器 + 告警去重
16. **H12** — create_notification 校验 user_id/id 非空
17. **H14** — Switch 添加 disabled 属性
18. **M1** — daily_aggregation 改为聚合昨天数据
19. **M2** — 维护模式 /status 子路径豁免
20. **M3** — device/maintenance 中间件使用 ErrorBody
21. **M4** — 广播长度校验改用 `chars().count()`
22. **M5** — complete_session 添加状态前置检查
23. **M6** — Cookie 添加 Max-Age + SameSite 可配置
24. **M9** — AMAS is_new_context 仅在 session 变更时触发
25. **M16/M17** — 登录时序泄漏修复
26. **M18** — 移除 refresh token 回退死代码
27. **M21** — SSP from_tables_with_bins 使用 config 值
28. **M22** — SWD SIMILARITY_CACHE 添加淘汰逻辑

### P2（计划修复）
29. **M7-M15, M19-M20, M23-M36** — 其余 Medium 级别问题
30. **L1-L29** — Low 级别问题
31. 硬编码值统一纳入 config 或 constants

---

## 九、待用户确认

1. **daily_aggregation 设计意图**：聚合"昨天"还是"今天到目前为止"？
2. **Cookie 有意设计为会话级？** 还是应持久化 refresh token cookie？
3. **未活跃用户是否应计入 at-risk？** 还是单独分类？
4. **etymology_generation 占位文本策略**：是否改为仅标记而不写入？或用 `generated: "placeholder"` 区分？
5. **广播标题/消息 200/2000 限制**：是指字符数还是字节数？
6. **注册时是否应强制 username 唯一？** 当前仅检查 email 唯一性。
7. **设备封禁 DB 不可用时的策略**：拒绝请求（fail-closed）还是放行（fail-open，当前行为）？
8. **SSE 未认证重连策略**：是否应停止重连直到用户登录？
9. **delayed_reward worker**：是否为计划中功能？当前为空存根，是否应移除或标注 TODO？
10. **algorithm_optimization worker**：当前仅调一个参数，是否应增强或禁用？
11. **heartbeat_watchdog**：是否应接入调度器？告警去重策略如何？
12. **SWD SIMILARITY_CACHE 淘汰策略**：LRU？TTL 扫描？最大容量？
