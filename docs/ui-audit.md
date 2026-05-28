# WordForge 前端 UI / 动画 / 交互静态审计

> 7 个并行 agent 仅通过阅读源码（不运行、不修改），对 admin 控制台 + 共享 UI 组件库 + 客户端占位页做完整静态审计。
> 全部基于 Solid.js 1.x 框架特性核对（`class=` / `createSignal` / `Show/For` / `onCleanup`）。
> 总计 **221** 条发现：**27 P0 / 98 P1 / 96 P2**。

## 目录

- [一、TL;DR P0 列表（必修，含业务/动画/可访问性阻断）](#一tldr-p0-列表必修含业务动画可访问性阻断)
- [二、按 agent 分域明细](#二按-agent-分域明细)
  - [Agent 1 — Admin 大页面 part 1](#agent-1--admin-大页面-part-1)
  - [Agent 2 — Admin 大页面 part 2](#agent-2--admin-大页面-part-2)
  - [Agent 3 — Probe + admin 小页面](#agent-3--probe--admin-小页面)
  - [Agent 4 — AMAS 子组件](#agent-4--amas-子组件)
  - [Agent 5 — 共享 UI 交互组件](#agent-5--共享-ui-交互组件)
  - [Agent 6 — 共享 UI 展示组件](#agent-6--共享-ui-展示组件)
  - [Agent 7 — Layout/Auth/Probe/Client 占位](#agent-7--layoutauthprobeclient-占位)
- [三、按问题域横向归并](#三按问题域横向归并)
- [四、统计](#四统计)

---

## 一、TL;DR P0 列表（必修，含业务/动画/可访问性阻断）

业务硬伤优先（影响功能正确性 / 安全 / 数据完整性）：

1. **ProbePage 危险脚本二次确认形同虚设** — `admin-ui/src/pages/admin/ProbePage.tsx:383-394` `onConfirmed` 仅本地过滤 results，**未调用 `/api/admin/probe/confirm`**，reload/clearCache/signOut 等 D 类脚本的二次确认完全空操作。
2. **AmasConfigPage 加载失败可推空配置** — `admin-ui/src/pages/admin/AmasConfigPage.tsx:36-47` 配置加载失败仅 toast，`config/baseline` 仍是 `{}`，热重载按钮 disabled 条件不含 `loadError`，**用户可一键把空对象推给后端覆盖运行时配置**。
3. **UpdatesPage SSE 与轮询双进度互相打架** — `admin-ui/src/pages/admin/UpdatesPage.tsx:105-129` apply 后同时跑 SSE + 2s 轮询，progress signal 被双向写入，进度条会**回跳**。
4. **UpdatesPage SSE 订阅位置错误** — `admin-ui/src/pages/admin/UpdatesPage.tsx:35-42` `connectSseStream` 写在组件函数体顶层而非 `onMount`，HMR/路由切换会重复 connect/cleanup。
5. **JsonAdvancedPanel 静默覆盖用户未保存编辑** — `admin-ui/src/pages/admin/amas/JsonAdvancedPanel.tsx:14,42-47` textarea 受控于 `JSON.stringify(props.config)`，外部 props 一变就把用户正在编辑的 textarea 内容覆盖，光标跳尾、选区丢失。
6. **JsonAdvancedPanel ref 透传未确认** — `admin-ui/src/pages/admin/amas/JsonAdvancedPanel.tsx:42` `textareaRef` 通过自定义 TextArea 组件传入，若 `Input.tsx` 未透传 ref，`applyText()` 永远拿到 undefined，"应用到表单"按钮**完全不工作**。
7. **AdminLoginPage 锁定倒计时不刷新** — `admin-ui/src/pages/admin/AdminLoginPage.tsx:46-78` 无 `setInterval` 驱动 UI，error 文案里的"等待 X 秒"会一直停在最初数字直到下一次点击才更新。
8. **SettingsPage 广播/更新通知按钮无 disabled** — `admin-ui/src/pages/admin/SettingsPage.tsx:268` `<Button>` 在 inflight 时未 disable，用户可连点开多重确认弹层、或重复广播。
9. **ClientsPage 整页 loading 卸载已展开面板** — `admin-ui/src/pages/admin/ClientsPage.tsx:138` `<Show when={!loading()}>` 包到 Tabs + Telemetry 之外，点"刷新"会强制把已展开的设备遥测面板关掉。
10. **AdminWordbookCenterPage 预览闪旧值** — `admin-ui/src/pages/admin/AdminWordbookCenterPage.tsx:319` Modal `open={showPreview()}` 与外层 `<Show when={preview()}>` 双重控制，关闭后 `preview()` 仍为旧值，下次打开如果 API 慢会先闪一帧旧标题再切新内容。
11. **AdminDashboard Empty 全失败才触发** — `admin-ui/src/pages/admin/AdminDashboard.tsx:316-321` `<Show when={a.error && b.error && c.error...}>` 6 个 resource 全部失败才提示，4/6 失败时页面只剩永远转圈的 Skeleton，用户无从判断。
12. **AnalyticsPage KPI 在 lg 断点失衡** — `admin-ui/src/pages/admin/AnalyticsPage.tsx:55` 6 卡仅 `xl:grid-cols-6`，缺 `lg:grid-cols-6`，1024-1279 区间 3×2 + sidebar 占位 + 长 KPI 标题严重挤压。
13. **AdminLayout sidebar 宽度与主区 ml 双套常量** — `admin-ui/src/components/layout/AdminLayout.tsx:151` 改一边另一边脱位（72/56/16 三档常量未集中管理）。

可访问性 / 动画对称 / 焦点陷阱：

14. **Modal 首焦点落在关闭按钮** — `admin-ui/src/components/ui/Modal.tsx:39-44` `FOCUSABLE_SELECTOR` 命中头部"×"按钮，回车直接关闭。
15. **Modal openModalCount 嵌套计数偏离** — `admin-ui/src/components/ui/Modal.tsx:32-88` cleanup 与 effect 重入交叉计数，多次开闭后引用计数不归零，body overflow 锁不释放。
16. **Toast 无出场动画** — `admin-ui/src/components/ui/Toast.tsx:33-63` 只有入场 motion，出场直接卸载节点，"瞬移消失"与"底部上推"撞车。
17. **Toast 计时器无 hover 暂停** — `admin-ui/src/components/ui/Toast.tsx:65-75` setTimeout 不 clear、不 pause-on-hover，鼠标停在 toast 上也会消失。
18. **AmasVersionDrawer 右滑入但直接卸载** — `admin-ui/src/components/admin/AmasVersionDrawer.tsx:72-83` enter 无 translate-x 动画，leave 直接 `Show` 卸载，进退完全不对称。
19. **PresetSelector / VersionDrawer 全屏遮罩无 ESC / focus trap / aria-modal** — `admin-ui/src/pages/admin/amas/PresetSelector.tsx:45`、`AmasVersionDrawer.tsx:109` 自绘 fixed inset-0 没用 Modal 组件，键盘用户死锁、屏阅读不识别为对话框。
20. **Tabs ARIA 残缺** — `admin-ui/src/components/ui/Tabs.tsx:33-44` 方向键直接触发 onChange、无 Home/End、无 aria-controls/id 关联面板。
21. **Table.tsx 横滚 + 缺 scope/caption** — `admin-ui/src/components/ui/Table.tsx:24,30` `overflow-x-auto` 与硬性要求"不可横滚"矛盾；`<th>` 缺 `scope="col"`，`<table>` 缺 `<caption>`。
22. **EChart 初始化双 setOption + 0 尺寸无回填 + 无 empty** — `admin-ui/src/components/ui/EChart.tsx:38-71` onMount 与 createEffect 都调 setOption；父级 display:none 初始尺寸 0 时无 resize 回退；series 为空无 Empty 占位。
23. **UpdateBanner 固定 top-0 z-40 与 sidebar/header 撞层 + 无 role="alert"** — `admin-ui/src/components/ui/UpdateBanner.tsx:8` 既挡内容又不被 SR 播报。
24. **ScriptEditor 父级 normalize 会覆盖用户输入光标** — `admin-ui/src/components/probe/ScriptEditor.tsx:57-65` createEffect 同步 props.value 回写，无 external flag 保护 IME 输入。
25. **ProtectedRoute 双重重定向** — `admin-ui/src/components/auth/ProtectedRoute.tsx:18-20` `navigate()` 与 fallback `<Navigate>` 双触发，闪烁/重复挂载。
26. **SystemLockedModal × MaintenancePage 同屏可叠加** — `admin-ui/src/components/SystemLockedModal.tsx:6` `z-50` 与 `admin-ui/src/pages/MaintenancePage.tsx:3` `z-[9999]` 概念重叠且互相覆盖，无互斥分发。
27. **EChart ResizeObserver / theme 切换 setOption{notMerge:true} 会丢业务字段** — `admin-ui/src/components/ui/EChart.tsx:43-67` 应改用 echarts 自带 theme 重建机制。

---

## 二、按 agent 分域明细

### Agent 1 — Admin 大页面 part 1

> 范围：AdminDashboard / AdminWordbookCenterPage / AnalyticsPage / ClientsPage

#### P0

- [ ] `admin-ui/src/pages/admin/AnalyticsPage.tsx:55` — 6 卡 KPI 缺 `lg:grid-cols-6` 兜底，lg 区间标题挤压。修复：补 `lg:grid-cols-6` 或 sm 改 `flex-wrap`。
- [ ] `admin-ui/src/pages/admin/AdminDashboard.tsx:316-321` — Empty 触发条件是"全部失败"组合 `&&`，部分失败时无降级提示。修复：拆为逐 Panel 内部 retry。
- [ ] `admin-ui/src/pages/admin/ClientsPage.tsx:138` — `<Show when={!loading()}>` 包到 Tabs/Telemetry 之外，刷新会卸载已展开面板。修复：loading boundary 仅包列表区。
- [ ] `admin-ui/src/pages/admin/AdminWordbookCenterPage.tsx:319` — Modal `open` 与外层 `Show when={preview()}` 双重控制导致闪旧值。修复：onClose 同时 `setPreview(null)`。

#### P1

- [ ] `admin-ui/src/pages/admin/AdminDashboard.tsx:259-266` — 状态点 `text-success` 加在 2×2px span 无效；degraded/error 仍 `animate-ring-pulse` 是噪声。修复：仅 healthy 脉动。
- [ ] `admin-ui/src/pages/admin/AdminDashboard.tsx:53-105` — KPI Skeleton 不带动画但回填后 StatCard 又跑 fade-in-up，肉眼一次错位闪动。修复：把动画放到内层数据回调上或 Skeleton 共用同款动画。
- [ ] `admin-ui/src/pages/admin/AdminWordbookCenterPage.tsx:233-288` — Card 整卡可点击但 `<div>` 缺 `role="button"`/`tabIndex`/键盘激活。修复：补 a11y 或改 button 语义。
- [ ] `admin-ui/src/pages/admin/ClientsPage.tsx:134-135` — 顶 toolbar 缺 `<h1>` 与其他 admin 页不一致。修复：补 `<h1 class="text-title">客户端</h1>`。
- [ ] `admin-ui/src/pages/admin/ClientsPage.tsx:181, 235` — `<td class="flex">` 覆盖 `display: table-cell`，按钮组与上方 `tabular-nums` 列基线错位。修复：flex 放到内层 div。
- [ ] `admin-ui/src/pages/admin/ClientsPage.tsx:122` — `loadTelemetry` 多 setState 未 `batch`，切换设备时"内容→骨架→新内容"三段闪烁。修复：`batch(() => {...})` 包裹。
- [ ] `admin-ui/src/pages/admin/AdminWordbookCenterPage.tsx:153-178` — Updates banner 用 `mt-3` 硬撑按钮组，多行词书名时按钮紧贴文字下沿。修复：banner column 布局或按钮放右侧固定区。
- [ ] `admin-ui/src/pages/admin/AnalyticsPage.tsx:282` — 饼图 `borderColor: '#fff'` 硬编码白色，深色主题被切马赛克。修复：用 cssVar token。
- [ ] `admin-ui/src/pages/admin/ClientsPage.tsx:88, 188, 241` — ban/refresh/requestTelemetry 按钮无 disabled/loading，连点发多次请求。修复：补 in-flight 标志。

#### P2

- [ ] `admin-ui/src/pages/admin/AdminWordbookCenterPage.tsx:194-212` — tag chip transition 只加 `colors`，hover/press 切换 background 跳变无 fade。
- [ ] `admin-ui/src/pages/admin/AdminDashboard.tsx:67, 81, 94` — 3 个 stagger wrapper inline style 重复，可抽 `.stagger-1/2/3`。
- [ ] `admin-ui/src/pages/admin/AnalyticsPage.tsx:206` — tooltip formatter 用 `(p: any)`，类型一致性弱。
- [ ] `admin-ui/src/pages/admin/ClientsPage.tsx:222, 267` — `new Date(str.replace(' ','T')+'Z')` 时间字符串两处重复 + ISO 容错差。
- [ ] `admin-ui/src/pages/admin/AdminWordbookCenterPage.tsx:269-274` — 导入按钮 disabled 阻止并行，但同步按钮（line 277）不受约束，可同时点导入 A + 同步 B。
- [ ] `admin-ui/src/pages/admin/AdminDashboard.tsx:297` — `truncate` 父无 `max-w` 约束，在 flex column 中依赖 `min-w-0` 可能不生效。
- [ ] `admin-ui/src/pages/admin/ClientsPage.tsx:266` — eventType `Badge variant="accent"` 与状态 Badge (`error`/`success`) 视觉权重相同，分类/状态混淆。
- [ ] `admin-ui/src/pages/admin/ClientsPage.tsx:273-275, 298-300, 314-316, 342-344` — 内联 svg icon 4 处硬编码且 stroke-width 不一致。

---

### Agent 2 — Admin 大页面 part 2

> 范围：AmasAdvisorPage / AmasConfigPage / MonitoringPage / UserManagementPage / UpdatesPage

#### P0

- [ ] `admin-ui/src/pages/admin/AmasConfigPage.tsx:36-47` — config 加载失败仅 toast，`config/baseline={}` 仍允许热重载推空对象覆盖后端运行时。修复：独立 `loadError` 信号，整页禁用所有操作按钮。
- [ ] `admin-ui/src/pages/admin/UpdatesPage.tsx:35-42` — `connectSseStream` 在组件函数体顶层而非 `onMount`，路由切换/HMR 会重复 connect。修复：放进 `onMount`，并核验 disconnect 返回类型。
- [ ] `admin-ui/src/pages/admin/UpdatesPage.tsx:105-129` — SSE 与 2s 轮询同时向 progress signal 写值，race 导致进度条回跳。修复：apply 后只信 SSE，仅在静默 N 秒时回落到轮询。

#### P1

- [ ] `admin-ui/src/pages/admin/AmasAdvisorPage.tsx:48` — `approve` 用浏览器原生 `confirm()` 显示多行 patch，与项目 `ConfirmDialog` 割裂。
- [ ] `admin-ui/src/pages/admin/AmasAdvisorPage.tsx:64` — `reject` 用原生 `prompt()` 收拒绝原因，无校验、移动端体验差。
- [ ] `admin-ui/src/pages/admin/AmasAdvisorPage.tsx:154-181` — 历史 6 列表格在窄屏直接撑破布局，缺 `overflow-x-auto` 父容器。
- [ ] `admin-ui/src/pages/admin/AmasAdvisorPage.tsx:246` — `<pre>` 缺 `overflow-y-auto`，长 evidence JSON 被截断且不能竖向滚动。
- [ ] `admin-ui/src/pages/admin/UpdatesPage.tsx:198-210` — outcome=failed/timeout 时 `setProgress(null)` 让进度条直接消失，缺保留最后一帧 + 失败态视觉。
- [ ] `admin-ui/src/pages/admin/UpdatesPage.tsx:206` — `transition-all duration-300` 让 width/color/radius 都参与过渡，SSE 帧间出现"卡顿后追赶"。修复：`transition-[width]`。
- [ ] `admin-ui/src/pages/admin/UpdatesPage.tsx:194` — 警告文本 `**未找到匹配当前架构的产物**` 是 markdown 语法但渲染在纯文本 `<p>`，星号原样显示。
- [ ] `admin-ui/src/pages/admin/UpdatesPage.tsx:135` — `setTimeout(reload, 1500)` 缺倒计时提示，用户可能在 1.5s 内点其他链接被打断。
- [ ] `admin-ui/src/pages/admin/UserManagementPage.tsx:65-76` — `setLoading(true)` 把整张表替换为 Spinner，翻页 flicker。修复：引入 `paging` 信号保留旧数据。
- [ ] `admin-ui/src/pages/admin/UserManagementPage.tsx:236, 248` — `key-result` 视图密钥可被关 Modal 直接丢失，无二次确认。
- [ ] `admin-ui/src/pages/admin/UserManagementPage.tsx:248` — 生成密钥按钮无防抖，快速双击发起两次重置。
- [ ] `admin-ui/src/pages/admin/UserManagementPage.tsx:236` — Modal title `${resetTarget()?.username ?? ''}` 在关闭瞬间会闪一帧空标题。
- [ ] `admin-ui/src/pages/admin/AmasConfigPage.tsx:130-135` — 热重载按钮 disabled 缺 `!dirty()`，无改动也能推送 baseline。
- [ ] `admin-ui/src/pages/admin/AmasConfigPage.tsx:163-171` — drawer 打开时如有未保存修改无提示，dirty 状态下 restore 历史版本会被覆盖。
- [ ] `admin-ui/src/pages/admin/MonitoringPage.tsx:74-93` — onMount 一次性 fetch，无 polling / 刷新按钮 / 上次更新时间戳。
- [ ] `admin-ui/src/pages/admin/MonitoringPage.tsx:46-58` — RecursiveKV `<details>` summary 自身无缩进，第 3+4 层视觉对齐到同一列；浏览器默认 disclosure 三角与 design system 不一致。

#### P2

- [ ] `admin-ui/src/pages/admin/AmasAdvisorPage.tsx:233` — patch 数值 `toFixed(6).replace(...)` 对科学计数值丢精度。
- [ ] `admin-ui/src/pages/admin/AmasAdvisorPage.tsx:86` — `grid-cols-2 sm:grid-cols-2` sm 冗余声明。
- [ ] `admin-ui/src/pages/admin/AmasAdvisorPage.tsx:131` — pending 空态描述把开发者文案暴露给管理员。
- [ ] `admin-ui/src/pages/admin/MonitoringPage.tsx:43` — `<For>` 多 object 平铺无视觉分隔。
- [ ] `admin-ui/src/pages/admin/MonitoringPage.tsx:138` — 公开健康探针无 last probed at 时间戳。
- [ ] `admin-ui/src/pages/admin/MonitoringPage.tsx:112, 140, 165` — 三处 grid 都写了 `grid-cols-2 sm:grid-cols-2 lg:grid-cols-N`，sm 重复 base。
- [ ] `admin-ui/src/pages/admin/UserManagementPage.tsx:179-230` — 无搜索/过滤组件，仅靠分页翻找。
- [ ] `admin-ui/src/pages/admin/UserManagementPage.tsx:288` — 复制按钮用 outline Button 占宽，建议改 IconButton。
- [ ] `admin-ui/src/pages/admin/UpdatesPage.tsx:11-12` — `POLL_*` 常量在 SSE 上线后名存实亡，需注释/移除。
- [ ] `admin-ui/src/pages/admin/UpdatesPage.tsx:229-231` — `<pre>` 渲染 releaseNotes 未限定 `max-h-`/`overflow-y-auto`。
- [ ] `admin-ui/src/pages/admin/UpdatesPage.tsx:238-241` — `安全提示` 反引号包路径但 `<li>` 是纯文本，反引号原样显示。
- [ ] `admin-ui/src/pages/admin/UpdatesPage.tsx:152` — `grid-cols-1 md:grid-cols-2` 两卡 md 以下高度不齐，缺 `auto-rows-fr`/`items-stretch`。
- [ ] `admin-ui/src/pages/admin/AmasConfigPage.tsx:117` — 按钮组 `flex flex-wrap` 在窄屏可能把"放弃修改"和"保存配置"拆到上下行。
- [ ] `admin-ui/src/pages/admin/AmasConfigPage.tsx:225` — 表格 hover `duration-fast ease-out-expo` 与其他表格 `duration-150` 不一致。

---

### Agent 3 — Probe + admin 小页面

> 范围：ProbePage / SettingsPage / FeedbackPage / AmasMetricsPage / AdminLoginPage / AdminSetupPage

#### P0

- [ ] `admin-ui/src/pages/admin/ProbePage.tsx:383-394` — `onConfirmed` 只过滤本地 results，**未调用真实 `/api/admin/probe/confirm`**，D 类危险脚本（reload/clearCache/signOut）二次确认形同虚设；同时 `open={true}` 写死、靠外层 Show 卸载，丢失关闭动画。
- [ ] `admin-ui/src/pages/admin/AdminLoginPage.tsx:46-78` — 锁定倒计时无 `setInterval` 驱动 UI 刷新，error 文案里的"等待 X 秒"卡住；loading() 期间 email/password 未 disabled。修复：interval + onCleanup + disabled。
- [ ] `admin-ui/src/pages/admin/SettingsPage.tsx:268` — "发送广播"按钮 inflight/showBroadcastConfirm 期间未 disabled，连点开多重确认；line 282 更新通知按钮同问题。

#### P1

- [ ] `admin-ui/src/pages/admin/ProbePage.tsx:50` — `handleSend` 重启 SSE 前未做 batchToken 校验，旧流 microtask 回调可能污染新批次结果。
- [ ] `admin-ui/src/pages/admin/ProbePage.tsx:280` — `Number(e.value)||3000` 清空瞬间被强制回填 3000，光标抖动；同问题 :230 maxPerDay/minConfidence。
- [ ] `admin-ui/src/pages/admin/ProbePage.tsx:251-255` — 模板下拉 `e.currentTarget.value=''` 直接 mutate DOM 绕过 controlled value，Safari reselect 同模板不再触发 change。
- [ ] `admin-ui/src/pages/admin/SettingsPage.tsx:122-285` — 整页无未保存离开提醒，maxUsers/defaultDailyWords 改动丢失。
- [ ] `admin-ui/src/pages/admin/SettingsPage.tsx:164` — `<Show when={settings()}>` 内层无 fallback，广播/更新 Card 在 settings 加载完前直接消失再"砰"地出现。
- [ ] `admin-ui/src/pages/admin/AdminLoginPage.tsx:102` — password Input 的 `error` 承载邮箱空/登录失败/锁定三类，邮箱错误也挂在密码框下。
- [ ] `admin-ui/src/pages/admin/AdminLoginPage.tsx:97` — 回车提交锁定期间 `getRemainingLockSeconds` 不会刷新，文案卡住（同 P0）。
- [ ] `admin-ui/src/pages/admin/AdminSetupPage.tsx:80` — 确认密码框承载邮箱/密码/setup 四类错误，定位错误。
- [ ] `admin-ui/src/pages/admin/FeedbackPage.tsx:53-89` — err fallback 内无"重试"按钮，且分页器在 fallback 外面，错误后操作链路断裂。
- [ ] `admin-ui/src/pages/admin/FeedbackPage.tsx:60-62` — 翻页时 stagger animation 让所有新行重新瀑布，造成强烈"翻页瀑布感"。

#### P2

- [ ] `admin-ui/src/pages/admin/ProbePage.tsx:351` — `md:sticky md:top-4 md:self-start` 历史侧栏缺 `max-height + overflow-y-auto`，满屏 sticky 部分被遮。
- [ ] `admin-ui/src/pages/admin/ProbePage.tsx:176` — `md:grid-cols-[1fr_300px]` 在 md 临界点左栏 ScriptEditor 触发横滚条。
- [ ] `admin-ui/src/pages/admin/ProbePage.tsx:407-413` — `formatTime` 仅 `HH:MM`，跨天 batch 混淆。
- [ ] `admin-ui/src/pages/admin/ProbePage.tsx:212-217` — `<Show>` 三态切换仅 fade-in 无 fade-out，进退不对称。
- [ ] `admin-ui/src/pages/admin/SettingsPage.tsx:175-241` — 5 个数字 Input 都 `||0` / `||20` 回填，清空抖动。
- [ ] `admin-ui/src/pages/admin/SettingsPage.tsx:202-244` — 两张 Card 各有"保存设置"但共用 `saving()`，点其一让另一也进 loading。
- [ ] `admin-ui/src/pages/admin/FeedbackPage.tsx:49` — Badge `共 X 条` 在 loading 初始 total=0 时显示"共 0 条"。
- [ ] `admin-ui/src/pages/admin/AmasMetricsPage.tsx:14-39` — 4 个 `<Show when={tab()===...}>` 平铺，切走面板直接卸载（无 keep-alive），切换无淡入。
- [ ] `admin-ui/src/pages/admin/AmasMetricsPage.tsx:23` — `onChange={(id) => setTab(id as TabId)}` 强转，Tabs 不约束 id 类型。
- [ ] `admin-ui/src/pages/admin/AdminLoginPage.tsx:80-110` — Login 卡片 `max-w-sm` 在 360px 旧 Android 上左右仅 8px padding，缺 `safe-area-inset-*`。
- [ ] `admin-ui/src/pages/admin/AdminLoginPage.tsx:99,102` — Email/Password Input 缺 `required` 属性；AdminSetupPage 74/77/80 三字段同问题。
- [ ] `admin-ui/src/pages/admin/AdminSetupPage.tsx:63-68` — checking 期间只渲染孤零零一个 Spinner，缺居中容器/文案。
- [ ] `admin-ui/src/pages/admin/AdminSetupPage.tsx:65-67` — checkError fallback 卡片无 `animate-fade-in-up`，与正常路径动画不一致。
- [ ] `admin-ui/src/pages/admin/ProbePage.tsx:242-265` — `<select>` 用 background-image base64 SVG 画下拉箭头，`stroke='currentColor'` 在 url(data:) 里不继承字色，dark mode 永远黑色。
- [ ] `admin-ui/src/pages/admin/ProbePage.tsx:319-323` — 结果区缺"清空结果"按钮。

---

### Agent 4 — AMAS 子组件

> 范围：pages/admin/amas/ 9 个 panel + components/admin/AmasVersionDrawer

#### P0

- [ ] `admin-ui/src/pages/admin/amas/JsonAdvancedPanel.tsx:14,42-47` — textarea 受控于 `JSON.stringify(props.config)`，外部一变就静默覆盖用户未提交编辑。
- [ ] `admin-ui/src/pages/admin/amas/JsonAdvancedPanel.tsx:42` — `textareaRef` 透自定义 TextArea 组件，若未 forwardRef，`applyText()` 永远 undefined，按钮不工作。
- [ ] `admin-ui/src/pages/admin/amas/PresetSelector.tsx:45`、`admin-ui/src/components/admin/AmasVersionDrawer.tsx:109` — 全屏遮罩 / Drawer / 确认弹窗均无 ESC / focus trap / `role="dialog" aria-modal` / body scroll lock。
- [ ] `admin-ui/src/components/admin/AmasVersionDrawer.tsx:72-83` — Drawer 右滑入但 enter/leave 完全无动画，背景 backdrop 只有 fade-in。
- [ ] `admin-ui/src/pages/admin/amas/JsonAdvancedPanel.tsx:14` — 每次 `props.config` 变化都全量 `JSON.stringify` 重赋 textarea value，光标跳尾、选区丢失。

#### P1

- [ ] `admin-ui/src/pages/admin/amas/MetricsDashboard.tsx:74-78` — 时间窗口按钮缺 `aria-pressed/aria-label/role="group"`；硬编码 `text-white` 与 AnomaliesPanel/UserStatePanel 风格不一致。
- [ ] `admin-ui/src/pages/admin/amas/SectionPanel.tsx:27` — `openSection` 默认锁死 `'memoryModel'`，PARAM_DICT 变化时首屏全折叠；展开无高度过渡（直接 `Show` 卸载/挂载）。
- [ ] `admin-ui/src/pages/admin/amas/SectionPanel.tsx:64` — 折叠状态指示器是文本"收起/展开"无 chevron；`aria-controls` 指向 `Show` 卸载后不存在的 panel。
- [ ] `admin-ui/src/pages/admin/amas/SectionPanel.tsx:26` — `errorMap = () => new Map(...)` 未 createMemo，每渲染重建 Map，O(N²)；`TierAPanel.tsx:14` 同问题。
- [ ] `admin-ui/src/pages/admin/amas/ParamField.tsx:21,29` — `const m = props.meta` 等价解构 props 破坏响应性；`explainCache.get(m.path)` 初始化时锁死 path。
- [ ] `admin-ui/src/pages/admin/amas/ParamField.tsx:50-54` — number input 超界不 clamp / 不显错误，外部 form 校验后用户看不到红框。
- [ ] `admin-ui/src/pages/admin/amas/ParamField.tsx:88-108` — number + slider 共写 `setValue → structuredClone(config)`，拖动滑块每像素都全 clone 295 字段对象。
- [ ] `admin-ui/src/pages/admin/amas/ParamField.tsx:99` — 原生 `<input type="range">` 无主题样式，仅 `accent-accent`；窄屏 + 中文长 label 会换行。
- [ ] `admin-ui/src/pages/admin/amas/ParamField.tsx:149` — 解释卡片用 emoji `🤖` 与"避免 emoji"规范及 token 体系不符。
- [ ] `admin-ui/src/pages/admin/amas/PresetSelector.tsx:46-52` — `max-w-2xl` + `max-h-[80vh]`，窄屏 diff 表 3 列挤压；表无 `overflow-x-auto`。
- [ ] `admin-ui/src/pages/admin/amas/PresetSelector.tsx:80` — "应用到表单"按钮 diff=0 时 disabled 与文案"以下字段将被覆盖（共 0 项）"重复。
- [ ] `admin-ui/src/components/admin/AmasVersionDrawer.tsx:82` — 关闭按钮 `×` 裸字符无 `aria-label`，命中区域 <24px。
- [ ] `admin-ui/src/components/admin/AmasVersionDrawer.tsx:42-51` — `createResource(expanded, ...)` 反复展开同一版本会反复打后端，无 hash 缓存。
- [ ] `admin-ui/src/components/admin/AmasVersionDrawer.tsx:96` — `diff={expanded() === v.versionHash ? diff() : []}` 每次都新建 array 引用，子树重建。修复：`createMemo`。
- [ ] `admin-ui/src/pages/admin/amas/VersionComparePanel.tsx:115` — 嵌套 `<Show>` 切换版本时 EChart 实例销毁创建抖动，应保留节点叠半透明 spinner。
- [ ] `admin-ui/src/pages/admin/amas/VersionComparePanel.tsx:104-109` — "还原/交换"按钮窄屏无 gap-wrap；交换 setter 依赖读取顺序，易读性差。
- [ ] `admin-ui/src/pages/admin/amas/VersionComparePanel.tsx:54-71` — 柱图 `barGap` 仅在 series[0] 设置；y 轴 `inverse: true` 让"事件数"反在底部，与文档顺序冲突。
- [ ] `admin-ui/src/pages/admin/amas/AnomaliesPanel.tsx:109` — `height="${...}px"` 字符串动态计算，需核实 EChart 组件是否在 prop 变化时 resize。
- [ ] `admin-ui/src/pages/admin/amas/UserStatePanel.tsx:41` — `axisLabel.fontSize: 9` 偏小，触屏密度差。
- [ ] `admin-ui/src/pages/admin/amas/UserStatePanel.tsx:114` — `<EChart option={() => histOption(h)}>` per item 闭包重建 option，每帧 echarts 全量重渲染。

#### P2

- [ ] `admin-ui/src/pages/admin/amas/AnomaliesPanel.tsx:67` — Spinner fallback 与最终 440px 图表高度差距大，加载切完成跳变。
- [ ] `admin-ui/src/pages/admin/amas/MetricsDashboard.tsx:86` — 同上。
- [ ] `admin-ui/src/pages/admin/amas/AnomaliesPanel.tsx:68` — `<Show>{(_) => ...}` 内部 `overview()!` 非空断言，建议 `Show keyed`。
- [ ] `admin-ui/src/pages/admin/amas/UserStatePanel.tsx:99` — 同上 `dist()!` 非空断言。
- [ ] `admin-ui/src/pages/admin/amas/ParamField.tsx:80,82` — Switch label "已启用/已停用" 文字切换时长度若改"开/关"会跳动。
- [ ] `admin-ui/src/pages/admin/amas/ParamField.tsx:113-146` — chip/影响/问AI/默认/已调优 5 个 inline 元素 `flex-wrap` 窄屏碎成 2-3 行。
- [ ] `admin-ui/src/pages/admin/amas/SectionPanel.tsx:32-39` — `sectionErrorCount` 每次渲染都 filter，O(N·M)，建议预聚合。
- [ ] `admin-ui/src/pages/admin/amas/PresetSelector.tsx:92` — `formatVal` 与 `AmasVersionDrawer.tsx:217` 完全重复，建议抽 util。
- [ ] `admin-ui/src/pages/admin/amas/VersionComparePanel.tsx:48-52` — `options()` 未 createMemo，每次 onChange 重算 50 行。
- [ ] `admin-ui/src/components/admin/AmasVersionDrawer.tsx:170` — diff 表缺 `<thead>`，与 PresetSelector 风格不一致。
- [ ] `admin-ui/src/components/admin/AmasVersionDrawer.tsx:74` — `Show when={confirming()}` 在 Drawer Show 内层，关闭 Drawer 时 confirming 不重置，下次打开仍旧值。
- [ ] `admin-ui/src/components/admin/AmasVersionDrawer.tsx:107` — confirm 模态 backdrop click 可能误关 Drawer，需核 click 冒泡。
- [ ] `admin-ui/src/pages/admin/amas/TierAPanel.tsx:29` — 11 卡 `lg:grid-cols-3` 最后一行 2 卡左对齐，视觉空白；建议 `xl:grid-cols-4`。

---

### Agent 5 — 共享 UI 交互组件

> 范围：Button / Modal / ConfirmDialog / Pagination / Select / Switch / Toast / Input

#### P0

- [ ] `admin-ui/src/components/ui/Modal.tsx:32-88` — effect / cleanup 与 `openModalCount` 嵌套计数交叉，多次开闭引用计数不归零，body overflow 锁不释放。修复：`createEffect(on(()=>props.open, ...))` + 顶层 onCleanup 兜底。
- [ ] `admin-ui/src/components/ui/Toast.tsx:65-75` — setTimeout 自动关闭无 hover/focus 暂停，组件销毁不清理。
- [ ] `admin-ui/src/components/ui/Toast.tsx:33-63` — 仅入场动画，出场直接卸载节点，瞬移消失。修复：`<Presence>` + exit。

#### P1

- [ ] `admin-ui/src/components/ui/Select.tsx:22-42` — 原生 `<select>` 但缺 `aria-invalid` / `aria-describedby` 关联 error 文本，与 `Input` 不一致；id 用模块级计数器。
- [ ] `admin-ui/src/components/ui/Select.tsx:14-15`、`admin-ui/src/components/ui/Input.tsx:12-16,71` — `let inputIdCounter = 0` / `selectIdCounter` 模块级单例，HMR/SSR/多实例 id 冲突。修复：`createUniqueId()`。
- [ ] `admin-ui/src/components/ui/Modal.tsx:94-103` — 遮罩 click 无关闭策略开关，loading/危险态下也可关闭。修复：`closeOnBackdrop?: boolean`。
- [ ] `admin-ui/src/components/ui/Modal.tsx:91-141` — `z-50` 硬编码字面量，与 Toast `z-[100]` 缺 token 协调。修复：design tokens `z-modal/z-dropdown/z-toast`。
- [ ] `admin-ui/src/components/ui/Modal.tsx:36,78` — 直接写 `document.body.style.overflow`，未保存还原原始值。修复：保存原值或 `data-scroll-locked` 属性 + CSS。
- [ ] `admin-ui/src/components/ui/Modal.tsx:39-44` — `FOCUSABLE_SELECTOR` 首选命中关闭按钮，打开后回车直接关。修复：排除关闭按钮或 fallback 到 dialog 容器。
- [ ] `admin-ui/src/components/ui/Switch.tsx:13-37` — `<button role="switch">` 嵌在 `<label>` 内，点 label 文字不触发；缺 `aria-labelledby`。
- [ ] `admin-ui/src/components/ui/Switch.tsx:21-24` — 轨道用 `transition-colors`，thumb 用 Motion spring，颜色与位移时序不同步。
- [ ] `admin-ui/src/components/ui/Pagination.tsx:17-31` — `c <= 3` 分支输出 `[1,2,3,...,t]`，中间页码丢失跳跃感强。
- [ ] `admin-ui/src/components/ui/Pagination.tsx:13` — `props.pageSize=0` 触发 `Math.ceil(total/0)=Infinity` 崩溃。
- [ ] `admin-ui/src/components/ui/Button.tsx:48-74` — loading 用 `disabled` 让按钮失焦，键盘流断。修复：`aria-busy` + `aria-disabled` + 手动忽略 onClick。
- [ ] `admin-ui/src/components/ui/Button.tsx:55,57` — `hover:-translate-y-px` 缺 `motion-reduce` / `@media (hover: none)` 兜底，移动端长按 hover 残留。

#### P2

- [ ] `admin-ui/src/components/ui/Button.tsx:65-69` — Spinner 与 icon 二选一布局会 4px 抖动，需固定尺寸容器包裹。
- [ ] `admin-ui/src/components/ui/Modal.tsx:101` — `backdrop-blur-md` + `bg-black/40` 双重开销，低端设备掉帧。
- [ ] `admin-ui/src/components/ui/Modal.tsx:135` — `max-h-[85vh] overflow-y-auto` 整 Modal 滚动，title 栏会滚出视口。
- [ ] `admin-ui/src/components/ui/Toast.tsx:34-44` — 新 toast 出现底部时旧 toast 不上滑，缺 layout 动画。
- [ ] `admin-ui/src/components/ui/Toast.tsx:68` — `aria-live="polite"` 对 error 不合适，应 `assertive` 或 `role="alert"`。
- [ ] `admin-ui/src/components/ui/Select.tsx:36-38` — placeholder `<option value="" disabled>` 缺 `selected`，用户无法选回 placeholder。
- [ ] `admin-ui/src/components/ui/Select.tsx:31` — 自定义箭头 background-image SVG `stroke=currentColor` 在 url(data:) 不继承字色，深色模式黑色。
- [ ] `admin-ui/src/components/ui/Input.tsx:30-47` — 受控但缺 onInput 时不可编辑无防呆；缺内建 `clearable` 支持。
- [ ] `admin-ui/src/components/ui/ConfirmDialog.tsx:30-35` — `hideClose=true` + 有 title 时 Modal `flex justify-between` 右侧留白。
- [ ] `admin-ui/src/components/ui/Pagination.tsx:34` — `totalPages() > 1` 不渲染整 nav，total=0/1 时父容器布局抖动。
- [ ] `admin-ui/src/components/ui/Pagination.tsx:36-45` — disabled `pointer-events-none` 多余，键盘 Tab 仍可聚焦无反应。

---

### Agent 6 — 共享 UI 展示组件

> 范围：Badge / Card / EChart / Empty / MiniStat / Panel / Progress / Skeleton / Spinner / StatCard / Table / Tabs / UpdateBanner / WindowPicker

#### P0

- [ ] `admin-ui/src/components/ui/Table.tsx:24` — `overflow-x-auto` 直接违反"不可横滚"硬性要求。修复：列优先级 + truncate，必要时仅 ≥md 关闭。
- [ ] `admin-ui/src/components/ui/EChart.tsx:38-70` — `containerRef` 写法 + onMount/createEffect 同时调 setOption，首屏双调用（merge + notMerge）。
- [ ] `admin-ui/src/components/ui/EChart.tsx:43-67` — 父级 `display:none` 时 init 测得 0×0 永远不重测；theme 切换用 `setOption({notMerge:true})` 丢业务字段，应用 echarts 自带 theme 重建。
- [ ] `admin-ui/src/components/ui/EChart.tsx:65-71` — 无 empty 状态，空 series 时只显空白方块。
- [ ] `admin-ui/src/components/ui/Tabs.tsx:33-44` — 方向键直接触发 onChange、无 Home/End、tab 缺 `id`、面板侧无 `aria-controls/labelledby`，违反 WAI-ARIA Tabs 模式。
- [ ] `admin-ui/src/components/ui/UpdateBanner.tsx:8` — `fixed top-0 z-40` 与 sidebar/header 撞层，banner 出现时遮第一行内容；缺 `role="alert"`。

#### P1

- [ ] `admin-ui/src/components/ui/Spinner.tsx:31` — `stroke-dasharray="47 63"` 与"270°/cap=round"声明不符（r=10 周长≈62.83，47 仅 75%）；track 与 spin ring 同半径产生毛刺。
- [ ] `admin-ui/src/components/ui/Spinner.tsx:21-25` — 缺 `aria-live="polite"` 与 SR 文本，aria-label 变化不会朗读。
- [ ] `admin-ui/src/components/ui/Skeleton.tsx:10-25` — 全 `aria-hidden="true"`，无 `role="status"` / `aria-busy`，对 AT 完全隐身。
- [ ] `admin-ui/src/components/ui/Skeleton.tsx:22-23` — `width` 无默认（undefined → 0 宽），调用方忘传直接看不到。
- [ ] `admin-ui/src/components/ui/Skeleton.tsx:32-34` — CardSkeleton 三行高度全 1rem，与真实 1.25rem 行高 + 行间距不匹配，布局抖动。
- [ ] `admin-ui/src/components/ui/Progress.tsx:38-46` — `transition-[width]` 首帧 0%→N% 闪烁；striped 浮层依赖未注释的全局 CSS 类。
- [ ] `admin-ui/src/components/ui/Progress.tsx:25-26` — `max=0` 时除零得 100%，缺保护。
- [ ] `admin-ui/src/components/ui/Progress.tsx:76-83` — CircularProgress 缺 `aria-valuetext` 与 `aria-label`。
- [ ] `admin-ui/src/components/ui/StatCard.tsx:82` — `text-3xl truncate` 长数字截尾，应去 truncate 改 compact (k/M)。
- [ ] `admin-ui/src/components/ui/StatCard.tsx:84-90` — trend.value=0 仍返回'持平'对象，语义混淆"无趋势"与"持平"。
- [ ] `admin-ui/src/components/ui/StatCard.tsx:68` — 默认 `variant="interactive"` 触发 hover 抬升 + cursor:pointer，但 StatCard 大多不可点。
- [ ] `admin-ui/src/components/ui/Tabs.tsx:84-86` — `useIndicatorTrack` 首测时间 race，indicator 从 0 滑到目标产生不必要入场动画。
- [ ] `admin-ui/src/components/ui/Tabs.tsx:50` — `ref={containerRef}` 类型与初始化时机需收紧（依赖 motion lib 内部 accessor 读取）。
- [ ] `admin-ui/src/components/ui/WindowPicker.tsx:5-9` — `value: () => number` 把 prop 设为函数，违反 Solid props 约定。
- [ ] `admin-ui/src/components/ui/WindowPicker.tsx:31-35` — Indicator `transform: translateX(left - 4)` 硬编码减 padding 4px，padding 改了就错位。
- [ ] `admin-ui/src/components/ui/WindowPicker.tsx:41-52` — 无键盘导航；语义应是 `radiogroup` + `role="radio" aria-checked` 不是 `aria-pressed`。
- [ ] `admin-ui/src/components/ui/Badge.tsx:55` — pulse 颜色 inline `color: CSS var`，与 `dotColorMap` 配色 token 解耦，dark mode 不一致。
- [ ] `admin-ui/src/components/ui/Empty.tsx:14-32` — 父容器 `animate-fade-in` 与子项 `animate-fade-in-up` 叠加，可见"接力闪烁"。
- [ ] `admin-ui/src/components/ui/Empty.tsx:32` — action slot wrapper `animate-fade-in-up` 与内部按钮 hover transform 在动画期间冲突。

#### P2

- [ ] `admin-ui/src/components/ui/Card.tsx:35-38` — 非 interactive variant 也开 transition，浪费计算。
- [ ] `admin-ui/src/components/ui/Card.tsx:8` — glass variant `backdrop-blur-md` Safari 兼容差，缺 `prefers-reduced-transparency` 降级。
- [ ] `admin-ui/src/components/ui/Panel.tsx:18` — `lg:grid-cols-4` aside 占 1 列，aside 为空时主区只占 3/4 留白。
- [ ] `admin-ui/src/components/ui/Panel.tsx:14-16` — `<h2>` 字号写死 `text-headline`，嵌套时层级语义错乱，需暴露 `headingLevel` prop。
- [ ] `admin-ui/src/components/ui/Table.tsx:30` — `<th>` 缺 `scope="col"`。
- [ ] `admin-ui/src/components/ui/Table.tsx:25` — `<table>` 缺 `<caption>`（即使 sr-only）。
- [ ] `admin-ui/src/components/ui/Table.tsx:62-69` — onRowClick 行可点击但缺 `role="button"` / `tabIndex` / Enter-Space 键盘触发。
- [ ] `admin-ui/src/components/ui/Table.tsx:38-52` — loading→data 切换无 fade，行高 `h-4` 与真实 `h-5` 不匹配跳变。
- [ ] `admin-ui/src/components/ui/MiniStat.tsx:28` — `text-2xl` 长数字仍截位，缺 compact format。
- [ ] `admin-ui/src/components/ui/MiniStat.tsx:26` — `hover:-translate-y-0.5` 但非可点击，应仅 `interactive` 时启用。
- [ ] `admin-ui/src/components/ui/UpdateBanner.tsx:8` — `animate-slide-down` 仅入场，关闭直接 unmount 跳变。
- [ ] `admin-ui/src/components/ui/UpdateBanner.tsx:11-22` — 两文字按钮无 `type="button"`，被 form 包裹会触发 submit。
- [ ] `admin-ui/src/components/ui/Progress.tsx:50-53` — 非整数 `value` 显示 `0.5/100` 不友好。
- [ ] `admin-ui/src/components/ui/Progress.tsx:43-46` — striped span 缺 `pointer-events-none`。
- [ ] `admin-ui/src/components/ui/StatCard.tsx:74` — dot 用 `colors().text.replace('text-','bg-')` 字符串替换依赖隐式 safelist。
- [ ] `admin-ui/src/components/ui/StatCard.tsx:69` — `flex items-start` 在 size='sm' + 长 title 多行时与 icon baseline 错位。
- [ ] `admin-ui/src/components/ui/Badge.tsx:41` — `whitespace-nowrap shrink-0` 长文本溢出无 `max-w` 兜底。
- [ ] `admin-ui/src/components/ui/Empty.tsx:26-29` — `h3` + `p` 间距 `mb-1` 过窄。
- [ ] `admin-ui/src/components/ui/Tabs.tsx:80-86` — indicator `bottom-0 h-0.5` 与容器 `border-b` 1px 叠加视觉弱化，应 `bottom-[-1px]`。
- [ ] `admin-ui/src/components/ui/Tabs.tsx:19` — 点击更新 active 但不重置 `focusedIndex`，下次方向键漂移。
- [ ] `admin-ui/src/components/ui/WindowPicker.tsx:11` — 无 loading/disabled 状态，异步切换时仍可点。

---

### Agent 7 — Layout/Auth/Probe/Client 占位

> 范围：AdminLayout / ProtectedRoute / probe 子组件 / ErrorBoundary / SystemLockedModal / Legacy/Maintenance/NotFoundPage

#### P0

- [ ] `admin-ui/src/components/auth/ProtectedRoute.tsx:18-20` — `navigate('/admin/login')` 与 fallback `<Navigate href="/admin/login">` 双重重定向闪烁/重复挂载。
- [ ] `admin-ui/src/components/probe/ScriptEditor.tsx:57-65` — createEffect 中 `view.dispatch` 在父级 normalize（trim/replace）时回写覆盖用户正在输入的内容，光标丢失 + IME 冲突。
- [ ] `admin-ui/src/components/SystemLockedModal.tsx:6` 与 `admin-ui/src/pages/MaintenancePage.tsx:3` — Modal `z-50` 与 Maintenance `z-[9999]` 概念重叠且可同屏叠加，缺 App 层互斥分发。

#### P1

- [ ] `admin-ui/src/components/layout/AdminLayout.tsx:151` — sidebar 宽度（72/56/16）与主区 ml class 是两套常量，改一边另一边脱位。
- [ ] `admin-ui/src/components/layout/AdminLayout.tsx:107-118` — Indicator `overflow-y-auto` 容器内若用 `offsetTop` 测量不随滚动，长 nav 滚动后激活态错位（需核 `useIndicatorTrack` 实现）。
- [ ] `admin-ui/src/components/layout/AdminLayout.tsx:165-167` — `<Show when={pageTitle()} keyed>` 同菜单子路由切换标题不变 → 标题不 fade，但 main 区按 pathname keyed 仍 fade，动画不同步。
- [ ] `admin-ui/src/components/auth/ProtectedRoute.tsx:34-43` — 后台 verify 失败直接 navigate 无 toast，用户切回标签页就被踢回登录页无解释。
- [ ] `admin-ui/src/components/probe/ConfirmDialog.tsx:44-46` — 裸 `div fixed inset-0 z-50` 没用 Modal 组件，无 ESC / focus trap / backdrop click 关闭。
- [ ] `admin-ui/src/components/probe/ConfirmDialog.tsx:60-68` — `<input>` 缺 id/`<label for>`、无 `aria-invalid`/`aria-describedby` 指向错误文案。
- [ ] `admin-ui/src/components/probe/ScriptEditor.tsx:18` — 无 dirty 状态对外暴露；容器无 `min-height`，CodeMirror 实例化前 50-100ms 布局塌陷。
- [ ] `admin-ui/src/pages/MaintenancePage.tsx:1-14` — 文案声称"自动恢复"但无轮询；齿轮静态无 animate-spin。修复：interval + onCleanup 探测 /api/system/health。
- [ ] `admin-ui/src/pages/NotFoundPage.tsx:5-12` — 缺 `<title>` 更新与 aria-labelledby；管理员误入 `/admin/*` 404 被踢出后台。
- [ ] `admin-ui/src/components/ErrorBoundary.tsx:26` — "返回首页"用 `window.location.href = '/'` 整页刷新丢未保存表单。
- [ ] `admin-ui/src/components/probe/ResultCard.tsx:73-75` — `bg-status-danger-light text-status-danger` 浅红底+红字 WCAG AA 不达标；`max-h-32` 长 stderr 无展开入口。

#### P2

- [ ] `admin-ui/src/components/layout/AdminLayout.tsx:69-76` — 移动端遮罩用 `<button>` 元素负责关闭，被 Tab 切到看不见的按钮扰乱焦点顺序。
- [ ] `admin-ui/src/components/layout/AdminLayout.tsx:119-146` — `<A>` 链接无 `aria-current="page"`。
- [ ] `admin-ui/src/components/layout/AdminLayout.tsx:136` — 折叠态 `md:w-16` + 图标 `group-hover:scale-110` 会让图标贴边/被裁。
- [ ] `admin-ui/src/components/layout/AdminLayout.tsx:107` — sidebar 无 footer 区，所有底部操作被挤到 header，与常见 admin 期望结构不一致。
- [ ] `admin-ui/src/components/probe/ConfirmDialog.tsx:46` — backdrop `bg-black/40` 与 SystemLockedModal 走 Modal 组件的风格不一致。
- [ ] `admin-ui/src/components/probe/ConfirmDialog.tsx:64` — `placeholder={expectedSuffix().replace(/./g, '·')}` 让人误以为是隐藏内容输入，UX 暗示错。
- [ ] `admin-ui/src/components/probe/ResultCard.tsx:46-53` — `copyJson` 失败静默吞错，用户无 toast 反馈。
- [ ] `admin-ui/src/components/probe/ResultCard.tsx:61-65` — pill 三段文本拼一 `<span>` 窄屏 wrap 打破 `flex justify-between`。
- [ ] `admin-ui/src/components/probe/ScriptEditor.tsx:33-44` — CodeMirror theme 编译一次，dark mode 切换不响应；与外层 div 双重圆角。
- [ ] `admin-ui/src/components/ErrorBoundary.tsx:8-9` — fallback 内 `console.error` 在每次 reset 重渲染时重复打印，无监控上报。
- [ ] `admin-ui/src/pages/LegacyUserFrontendPage.tsx:16-18` — `window.location.pathname` 不响应路由变化，应 `useLocation()`。
- [ ] `admin-ui/src/pages/LegacyUserFrontendPage.tsx:42` — fallback 文案暴露 `VITE_USER_APP_URL` 环境变量名给最终用户，应用 warning 色调。
- [ ] `admin-ui/src/pages/LegacyUserFrontendPage.tsx:32` — 反引号包 `` `wordforge-web` `` 直接展示为纯文本反引号。
- [ ] `admin-ui/src/pages/MaintenancePage.tsx:3` — `z-[9999]` 任意魔数，未协调 token 体系。
- [ ] `admin-ui/src/components/SystemLockedModal.tsx:6` — `onClose={() => {}}` + hideClose 完全屏蔽 ESC，对键盘用户死胡同。
- [ ] `admin-ui/src/components/auth/ProtectedRoute.tsx:7,13` — `VALIDATION_THROTTLE_MS = 30_000` 写死，与 token 实际过期无关联。

---

## 三、按问题域横向归并

跨域共性问题（同一类型在多文件复现，建议统一治理）：

### 3.1 焦点陷阱 / ESC / aria-modal 缺失（4 处自绘 Modal）

- `admin-ui/src/pages/admin/amas/PresetSelector.tsx:45`
- `admin-ui/src/components/admin/AmasVersionDrawer.tsx:109`
- `admin-ui/src/components/probe/ConfirmDialog.tsx:44-46`
- `admin-ui/src/components/ui/Modal.tsx:39-44`（首焦点落关闭按钮）

**统一治理**：禁止业务方自绘 fixed inset-0 弹层；强制走 `components/ui/Modal.tsx`，并在 Modal 内修首焦点策略 + 焦点陷阱 + ESC + body scroll lock。

### 3.2 入场动画但缺出场动画（瞬移消失）

- `admin-ui/src/components/ui/Toast.tsx:33-63`
- `admin-ui/src/components/ui/UpdateBanner.tsx:8`
- `admin-ui/src/components/admin/AmasVersionDrawer.tsx:72-83`
- `admin-ui/src/pages/admin/ProbePage.tsx:212-217`
- `admin-ui/src/pages/admin/amas/SectionPanel.tsx:27`

**统一治理**：引入 `solid-transition-group` 的 `<Presence>` 或 `solid-motionone` 的 exit；项目层规定凡有 enter 动画的组件必须配对 leave。

### 3.3 受控 Input 清空被默认值回填（输入抖动）

- `admin-ui/src/pages/admin/ProbePage.tsx:230, 280`（maxPerDay/minConfidence/timeoutMs 3 字段）
- `admin-ui/src/pages/admin/SettingsPage.tsx:175-241`（maxUsers/defaultDailyWords 等 5 字段）

**统一治理**：所有数字 Input 用独立 string signal 缓存中间态，submit/blur 时再 `Number(...)` 校验。

### 3.4 按钮 inflight 无 disabled

- `admin-ui/src/pages/admin/ClientsPage.tsx:88, 188, 241`
- `admin-ui/src/pages/admin/SettingsPage.tsx:268, 282`
- `admin-ui/src/pages/admin/UserManagementPage.tsx:248`

**统一治理**：危险/远程动作按钮统一引入 `useAsyncAction` hook 包装，inflight 期间自动 disabled + Spinner。

### 3.5 Modal title 在关闭瞬间闪空

- `admin-ui/src/pages/admin/UserManagementPage.tsx:236`
- `admin-ui/src/pages/admin/AdminWordbookCenterPage.tsx:319`

**统一治理**：Modal 组件内缓存上一次 title，仅在 open=true 切换时更新。

### 3.6 EChart / Echarts 性能与生命周期

- `admin-ui/src/components/ui/EChart.tsx:38-71`（双 setOption / 0 尺寸 / 无 empty / theme 切换丢字段）
- `admin-ui/src/pages/admin/amas/UserStatePanel.tsx:114`（per item 闭包 option 重建）
- `admin-ui/src/pages/admin/amas/VersionComparePanel.tsx:115`（切版本时销毁创建抖动）
- `admin-ui/src/pages/admin/amas/AnomaliesPanel.tsx:109`（动态 height 字符串）

**统一治理**：EChart 组件加 `option` createMemo 检测 + `theme` prop + `empty` 插槽 + ResizeObserver 触发 resize。

### 3.7 Solid 响应性陷阱（props 解构 / 漏加 `()` / signal 当函数 prop 传）

- `admin-ui/src/pages/admin/amas/ParamField.tsx:21,29`（`const m = props.meta` 等价解构）
- `admin-ui/src/components/ui/WindowPicker.tsx:5-9`（`value: () => number` 反模式）
- `admin-ui/src/pages/admin/amas/SectionPanel.tsx:26`、`TierAPanel.tsx:14`（errorMap 未 createMemo）

**统一治理**：ESLint `solid/no-destructure` + `solid/reactivity` 严格规则；项目内禁止 `value: () => T` 风格的 props。

### 3.8 模块级单例 id 计数器

- `admin-ui/src/components/ui/Input.tsx:12-16,71`
- `admin-ui/src/components/ui/Select.tsx:14-15`

**统一治理**：所有 UI 组件 id 用 Solid 的 `createUniqueId()`。

### 3.9 文本中的 markdown 语法直接显示

- `admin-ui/src/pages/admin/UpdatesPage.tsx:194`（`**...**` 星号原样显示）
- `admin-ui/src/pages/admin/UpdatesPage.tsx:238-241`（反引号原样显示）
- `admin-ui/src/pages/LegacyUserFrontendPage.tsx:32`（反引号 `wordforge-web`）

**统一治理**：项目内禁止在 JSX 文本节点用 markdown 语法；必要时用 `<strong>` / `<code>`。

### 3.10 grid 断点冗余声明

- `admin-ui/src/pages/admin/AmasAdvisorPage.tsx:86`
- `admin-ui/src/pages/admin/MonitoringPage.tsx:112, 140, 165`

**统一治理**：所有 `grid-cols-N sm:grid-cols-N` sm 冗余声明删除。

---

## 四、统计

| Agent | 文件数 | P0 | P1 | P2 | 小计 |
|------|------|----|----|----|----|
| 1 — Admin 大页面 part 1 | 4 | 4 | 9 | 8 | 21 |
| 2 — Admin 大页面 part 2 | 5 | 3 | 16 | 13 | 32 |
| 3 — Probe + admin 小页面 | 6 | 3 | 10 | 15 | 28 |
| 4 — AMAS 子组件 | 10 | 5 | 21 | 13 | 39 |
| 5 — 共享 UI 交互组件 | 8 | 3 | 12 | 10 | 25 |
| 6 — 共享 UI 展示组件 | 14 | 6 | 19 | 21 | 46 |
| 7 — Layout/Auth/Probe/Client 占位 | 10 | 3 | 11 | 16 | 30 |
| **合计** | **57** | **27** | **98** | **96** | **221** |

### 修复优先级建议

1. **第一波（业务正确性 P0）**：#1 ProbePage 确认未调后端、#2 AmasConfigPage 空对象推送、#3 UpdatesPage SSE/轮询双进度、#5 JsonAdvancedPanel 覆盖编辑、#7 AdminLoginPage 倒计时不动 —— 这 5 条直接影响数据完整性或用户被欺诈/打断，需最先修。
2. **第二波（动画/焦点对称 P0）**：#14-#22 Modal/Toast/Tabs/EChart/Drawer 一系列共享组件问题 —— 这些一旦修好可消化掉 Section 3.1/3.2 横向归并里的大批跨域 P1。
3. **第三波（横向治理）**：Section 3 中 10 类共性问题逐项收敛，预计可一次性消掉 30-40 条独立 finding。
4. **第四波（P2 打磨）**：颜色 token / 文案 / 一致性，安排到设计语言迭代里。
