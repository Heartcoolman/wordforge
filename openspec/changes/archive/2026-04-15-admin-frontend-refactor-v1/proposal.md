# Proposal: 管理员前端重构——专业化 · 可视化 · 美观

## Why

当前管理员前端（`frontend/src/pages/admin/` 及 `frontend/src/components/layout/AdminLayout.tsx`）存在以下问题，制约了其专业性与可用性：

1. **原始 JSON 直接暴露**：`MonitoringPage.tsx` 和 `AmasConfigPage.tsx` 将后端响应以 `<pre>` 块直接渲染，认知负担极高，完全不符合专业管理后台标准。
2. **零可视化**：项目当前无任何图表库。`AdminDashboard.tsx` 和 `AnalyticsPage.tsx` 仅展示孤立数字，无趋势感知，无法辅助管理决策。
3. **Layout 信息密度低**：`AdminLayout.tsx` 的 Header 固定写死"管理后台"，无面包屑、无当前页标题，用户空间感弱。
4. **KPI 卡片缺乏视觉层次**：`AdminDashboard.tsx` 中的统计卡片仅有大数字和小标签，无图标、无颜色区分、无趋势指示器，视觉过于平淡。
5. **ClientsPage 遥测面板信息结构混乱**：遥测详情以小字堆叠，关键字段（设备信息/会话/行为）没有视觉分组和层次。

## What Changes

### 依赖变更
- 新增前端依赖：`echarts`（Apache ECharts，用于折线图 / 柱状图 / 迷你 Sparkline）
- 不引入额外的 SolidJS ECharts wrapper 库，使用 ECharts 原生 API + SolidJS `onMount` / `onCleanup` 手动管理实例

### 新增后端 API（时序数据）
为支持趋势图，需在 `/api/admin/analytics/` 下新增两个端点：

- `GET /api/admin/analytics/daily-active-users?days=7`  
  返回：`[{ date: "2026-04-09", count: 12 }, ...]`（最近 N 天每日活跃用户数）
- `GET /api/admin/analytics/daily-records?days=7`  
  返回：`[{ date: "2026-04-09", correct: 340, total: 410 }, ...]`（最近 N 天每日学习记录）

若上述接口暂未实现，前端趋势图组件需能接受 `null` 数据并降级为骨架占位（`Skeleton`），不影响其他模块渲染。

### 新增前端共享组件（`frontend/src/components/ui/`）
- **`StatCard.tsx`**：KPI 统计卡片，props：`{ title, value, icon: SvgPath, color: 'accent'|'success'|'warning'|'error'|'info', trend?: { value: number, label: string } }`
- **`EChart.tsx`**：ECharts 实例包装器，props：`{ option: EChartsOption, class?: string, height?: string }`，内部负责 init / setOption / resize / dispose 生命周期管理

### 修改文件清单

#### `frontend/src/components/layout/AdminLayout.tsx`
- Header 由静态"管理后台"改为展示当前路由对应的页面标题（根据 `useLocation().pathname` 查表 `sidebarLinks`）
- Header 右侧加入当前管理员邮箱（调用 `adminApi.verifyToken()` 获取，缓存在 layout 级别 signal）和退出按钮（从底部挪至 header 右侧）
- 侧边栏底部退出按钮位置：保留侧边栏底部按钮不变（保持现有折叠态兼容）

#### `frontend/src/pages/admin/AdminDashboard.tsx`
- 统计数字区域：将现有 3 个普通 Card 替换为 3 个 `StatCard` 组件，各自带对应图标 SVG path 和颜色（用户→accent，单词→info，记录→success）
- 系统状态区域：将现有 4 格文字网格升级为结构化指标行——状态加 `StatusDot`（绿/红），运行时间格式化为 `Xd Xh Xm`，版本号旁保留"新版本"链接
- 新增趋势图区域：调用 `GET /api/admin/analytics/daily-active-users?days=7` 渲染 ECharts 折线图（日期为 X 轴，人数为 Y 轴）；若接口返回 null 则展示 Skeleton

#### `frontend/src/pages/admin/AnalyticsPage.tsx`
- 用户活跃度：3 个 `StatCard`（总用户/今日活跃/日活跃率），各带图标和颜色
- 学习数据：4 个 `StatCard`（总单词/总记录/正确数/总正确率）
- 新增图表区域：调用 `GET /api/admin/analytics/daily-records?days=7` 渲染 ECharts 柱状图（正确数和总记录数并列显示）；接口缺失时降级为 Skeleton

#### `frontend/src/pages/admin/MonitoringPage.tsx`
- **完全移除**所有 `<pre>{JSON.stringify(...)}</pre>` 展示
- **系统健康卡片**：状态（StatusDot + 文字）、版本、运行时间、数据库大小——4 格网格，每格带标签和值
- **公开健康探针卡片**：展示 `status` 字段及各子服务状态（用 Badge `success`/`error` 渲染）
- **数据库信息卡片**：展示 `sizeBytes`（格式化为 MB）、`pageSize`、`pageCount`、`walEnabled` 等关键字段，每项带描述标签
- **AMAS 监控事件卡片**：保留事件列表，但每条事件以结构化行展示（时间戳 + eventType Badge + 核心数值），移除 raw JSON

#### `frontend/src/pages/admin/AmasConfigPage.tsx`
- 配置编辑器 textarea 保持不变（raw JSON 编辑是合理的 DevTool 设计）
- **算法指标卡片**：移除 `<pre>` 展示，将 `metrics()` 对象的 top-level key-value 以两列网格渲染（key 为标签，value 为数值），嵌套对象折叠为可展开行（`<details><summary>`）

#### `frontend/src/pages/admin/ClientsPage.tsx`（minor）
- 遥测详情面板（`TelemetrySummary` 展示区）：已有分组卡片结构，微调：为"设备信息"/"会话统计"/"行为摘要"/"功能使用"各组加图标前缀和标题颜色，提升可读性
- 表格行 hover 加 `hover:bg-surface-secondary/50` 效果

#### `frontend/src/pages/admin/UserManagementPage.tsx`（minor）
- 表格 `<tr>` 加 `hover:bg-surface-secondary/40 transition-colors`
- 操作列按钮间距调整为 `gap-2`（现为 `space-x-2`，与其他页统一）

#### `frontend/src/pages/admin/AdminLoginPage.tsx`（minor）
- Card 上方加 Logo 占位区（16×16 accent 色圆形 icon + "WordForge Admin" 文字），替换现有"管理后台登录"文字标题

### 不修改的文件
- `frontend/src/pages/admin/SettingsPage.tsx`：表单设计已清晰，不在本次重构范围
- `frontend/src/pages/admin/AdminWordbookCenterPage.tsx`：卡片网格设计已合理，不在本次重构范围
- `frontend/src/pages/admin/AdminSetupPage.tsx`：初始化向导，低频使用，不在本次重构范围
- 所有 `frontend/src/api/` 文件（除新增后端 API 对应的调用）
- 所有 `frontend/src/components/ui/` 现有文件（仅新增，不修改）
- `frontend/src/index.css`（不修改主题变量）

## Capabilities

### New Capabilities

- `stat-card`：带图标和颜色的 KPI 统计卡片组件，用于仪表盘和分析页
- `echart-wrapper`：SolidJS 兼容的 ECharts 实例包装器，支持响应式 resize 和自动 dispose
- `trend-charts`：仪表盘日活跃用户折线图、分析页日学习记录柱状图（依赖新后端时序 API）
- `structured-monitoring`：系统监控页结构化展示——状态指示器、指标网格、事件列表，完全替代 raw JSON
- `structured-amas-metrics`：AMAS 配置页算法指标结构化展示

### Modified Capabilities

- `admin-layout`（`AdminLayout.tsx`）：Header 新增动态页面标题和管理员邮箱展示
- `admin-dashboard`（`AdminDashboard.tsx`）：KPI 卡片升级 + 趋势图新增
- `admin-analytics`（`AnalyticsPage.tsx`）：KPI 卡片升级 + 趋势图新增

## Impact

**前端**：
- `frontend/package.json`：新增 `echarts` 依赖
- `frontend/src/components/ui/StatCard.tsx`：新建
- `frontend/src/components/ui/EChart.tsx`：新建
- `frontend/src/components/layout/AdminLayout.tsx`：修改
- `frontend/src/pages/admin/AdminDashboard.tsx`：修改
- `frontend/src/pages/admin/AnalyticsPage.tsx`：修改
- `frontend/src/pages/admin/MonitoringPage.tsx`：大改（移除 pre，引入结构化组件）
- `frontend/src/pages/admin/AmasConfigPage.tsx`：修改指标展示区
- `frontend/src/pages/admin/ClientsPage.tsx`：小改（样式优化）
- `frontend/src/pages/admin/UserManagementPage.tsx`：小改（样式优化）
- `frontend/src/pages/admin/AdminLoginPage.tsx`：小改（Logo 区域）
- `frontend/src/api/admin.ts`：新增 `getDailyActiveUsers` 和 `getDailyRecords` 方法

**后端**：
- 新增路由 `GET /api/admin/analytics/daily-active-users`（路由文件：`src/routes/admin/analytics.rs` 或对应位置）
- 新增路由 `GET /api/admin/analytics/daily-records`
- 对应 store 查询方法（SQLite 按日期 GROUP BY）

**数据库**：无 schema 变更（基于现有 `learning_records` / `users` 表做聚合查询）

**测试**：
- `frontend/tests/components/ui/StatCard.test.tsx`：新建单元测试
- `frontend/tests/components/ui/EChart.test.tsx`：新建（仅测试 mount/unmount 不崩溃，不测试 canvas 渲染）
- `frontend/e2e/admin.spec.ts`：现有 E2E 测试中仪表盘/监控/分析页断言需更新（DOM 结构变更）

## Constraints

- **不引入 SolidJS ECharts wrapper 库**：实现时必须通过 web 搜索验证 ECharts 原生 API 与 SolidJS `onMount`/`onCleanup`/`createEffect` 的集成方式，禁止基于一般知识推断
- **不修改 Tailwind 主题变量**（`index.css`）：所有颜色使用现有 `--accent`、`--success`、`--info`、`--warning`、`--error` 语义变量
- **不破坏现有功能**：重构仅涉及展示层，所有 API 调用逻辑、状态管理、错误处理均保留
- **ECharts bundle 大小**：使用按需引入（`echarts/core` + 具体组件），不引入完整 bundle，控制产物体积
- **降级策略**：所有新增趋势图组件在数据为 null/加载中时展示 `Skeleton`，保证页面不因新 API 未实现而白屏
