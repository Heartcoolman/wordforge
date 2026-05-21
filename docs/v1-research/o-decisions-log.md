# V1.0 开放决策点拍板日志

> 起草日期：2026-05-21（Phase 0）
> 拍板人：team-lead（团队 `wordforge-v1` 调度长）
> 范围：`RFC.md §8` 倾向项 O1–O10 的最终选项
> 规则：按 RFC §8 给出的倾向项默认执行；后续任务实施过程中如出现强反证，本文件追加修订日志。

## 拍板汇总表

| # | 决策点 | 选项 | 本次拍板 | 简要理由 |
|---|---|---|---|---|
| O1 | M0-D7 Runbook 5 篇是否一次性 3 人日完成 | (a) 5 篇齐发 / (b) 按事故倒推 | **a — 一次性 5 篇齐发** | 避免事故时才发现缺；M0-D7 已估时 3.0 人日含全部 5 篇 |
| O2 | M0-C2 utoipa 注解链覆盖范围 | (a) 全量 / (b) 先 stable 档 25 个 | **b — 先 stable 档 25 个端点** | beta/internal 后补；GA 阻塞面最小化 |
| O3 | M1-G1 GDPR 导出格式 | (a) JSON Lines / (b) ZIP / (c) SQLite | **a — JSON Lines** | 单流响应、便于机器读；与 backlog 描述一致 |
| O4 | M1-G2 LLM 月度硬上限默认值 | (a) ¥50 / (b) ¥200 / (c) admin 设置 | **c — admin 设置 + 默认 ¥100** | 灵活 + 安全兜底；migration 加 `config` 行 default 100 |
| O5 | M0-R2 minisign 公钥嵌入方式 | (a) 编译期常量 / (b) 运行时 fetch | **a — 编译期常量（build.rs 注入）** | 最强保证；GH 账号被入侵无法替换公钥 |
| O6 | M1-A7 queryClient 删除后请求缓存方案 | (a) 自实现 cache map / (b) 仅 admin 大表加回 query | **a — 自实现 cache map（按需）** | 与 D10 一致；当前 createResource 架构已够用 |
| O7 | M2-Q1 k6 脚本入仓策略 | (a) 入仓周跑 / (b) 入仓仅 dispatch / (c) 不入仓 | **a — 入仓周一 03:00 跑** | 自动捕回归；CI workflow `.github/workflows/load-test.yml` |
| O8 | API 弃用窗口长度 | (a) 6 个月 / (b) 12 个月 / (c) 24 个月 | **a — 6 个月（≥ 2 个 minor）** | 与 §10.2 一致；`/api/v1/*` 410 已按 12 个月 sunset 写入因属"立即 410" 而非 deprecation |
| O9 | v1.0-rc.X 公开通道 | (a) 仅 beta 通道 / (b) stable+beta 双发 | **a — 仅 beta 通道** | 避免误升级用户群；release.yml `prerelease: true` 锁死 |
| O10 | M2-Q4 "无 P0 回归"判定 | (a) 仅 issue tracker / (b) issue + admin SSE + metrics 阈值 | **b — 三源合一** | 单源易漏；监控脚手架 `scripts/observe-rc.sh` 实现三源汇总 |

## 落地映射到 backlog 任务

- O1 → M0-D7（id=17）
- O2 → M0-C2（id=2）
- O3 → M1-G1（id=31）
- O4 → M1-G2（id=32）
- O5 → M0-R2（id=8）
- O6 → M1-A7（id=30）
- O7 → M2-Q1（id=34）
- O8 → M0-C4（id=4）/ M0-C5（id=5）
- O9 → M2-Q4（id=37）
- O10 → M2-Q4（id=37）

## 修订日志

- 2026-05-21：team-lead 初版按 RFC §8 倾向项默认全部拍板，无翻盘。
