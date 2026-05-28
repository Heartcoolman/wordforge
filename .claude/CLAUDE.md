# 语言规则

**⚠️ 必须始终使用简体中文与用户对话，禁止使用英文回复。**

以下场景也**必须使用中文**，不得切换为英文：

1. **解释代码逻辑和技术概念时** —— 用中文解释，专有术语可保留英文原文但需附中文说明
2. **报告错误和警告时** —— 用中文描述问题和解决方案，错误原文可引用但分析必须用中文
3. **Git commit 信息和 PR 描述** —— 除非用户明确要求英文，否则用中文撰写
4. **代码注释** —— 除非用户明确要求英文，否则用中文编写
5. **Markdown 标题和段落** —— 全部使用中文
6. **总结和汇报工作进展时** —— 使用中文
7. **提问和确认时** —— 使用中文

**唯一允许出现英文的地方：**
- 代码本身（变量名、函数名、关键字等）
- 命令行指令和文件路径
- 技术术语首次出现时可附英文原文，如：组件（Component）

---

# 项目术语

| 术语 | 定义 | 反义/混淆词 |
|---|---|---|
| **admin GUI / admin-ui** | 后端内嵌的管理员 Web 界面。源码在 `admin-ui/`，构建产物落仓库根 `static/`，由 `learning-backend` 二进制通过 `tower-http::ServeDir` 服务。**不是独立客户端**，没有独立部署形态 —— 离开后端二进制就不存在。 | 不要叫"前端"/"frontend"/"admin 客户端"/"admin SPA"（架构上确是 SPA，产品上是后端运维面） |
| **client / 客户端 / 设备** | 通过 `/api/*` 接入后端的 end-user 实体（Web / iOS / Android），DB 表 `client_devices`。**admin token 持有者不算客户端**。UI 文案统一使用"设备"，DB 字段沿用 `client_*`（迁移成本考量） | 不要把 admin 当 client |
| **wordforge-web** | end-user 学习端 SPA，独立 GitHub 仓库（`Heartcoolman/wordforge-web`），独立部署。本仓库**不**含其源码 | 不要在本仓找 wordforge-web 代码 |
| **learning-backend** | Rust crate 名 + 单二进制产物名（CI artifact / install.sh 里别名 `wordforge`） | — |
| **AMAS** | Adaptive Mastery Acquisition System，后端核心引擎 | — |

# 目录约定

- `src/` — Rust 后端 crate `learning-backend`
- `admin-ui/` — 后端的内嵌管理 GUI（SolidJS），构建产物落仓库根 `static/`
- `static/` — admin-ui 构建产物 + 资源包 `packs/`（由 `.gitignore` 排除子内容）
- `crates/` — Rust workspace 子 crate（如 `visual-fatigue-wasm`）
- `tests/` — 后端集成测试
- `docs/` — VitePress 文档站源码
