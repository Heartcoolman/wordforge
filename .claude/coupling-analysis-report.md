# 前端耦合度分析报告（关键任务 #3）

**分析日期**：2026-04-10  
**分析范围**：SolidJS 前端与后端 Rust/Axum 的耦合关系  
**核心问题**：当 iOS/Web/iPadOS 等不同客户端连接到部署的后端时，会丧失哪些功能？

---

## 执行总结

经过系统分析，发现 SolidJS 前端存在 **一个核心客户端专属功能**：
- **视觉疲劳检测（Visual Fatigue Detection）** 

其他所有功能（学习、单词管理、统计、AMAS 算法）都通过后端 API 完整暴露，**不同客户端可以完全复现**。

---

## 1. 丧失的功能详解

### 1.1 视觉疲劳检测系统（100% 客户端专属）

**功能描述**：
- 实时监测用户学习过程中的疲劳状态
- 使用摄像头捕捉面部特征，通过 MediaPipe 进行面部关键点检测
- 使用 Rust WASM 模块计算疲劳指标

**后端 SPA 路由配置**：
- 后端支持 `API_ONLY` 模式（`src/config.rs` 配置环境变量）
- 当 `API_ONLY=true` 时：禁用 SPA 静态资源服务，仅提供 REST API 路由
- 当 `API_ONLY=false` 时（默认）：提供完整的 SPA + API
- **重点**：无论模式如何，疲劳检测功能都是客户端侧实现，后端仅接收数据

**技术架构**：
```
用户摄像头 (HTMLMediaElement)
  ↓ getUserMedia()
CameraManager (前端本地)
  ↓ 每 100ms 捕捉一帧
Web Worker (独立线程)
  ↓ 加载 MediaPipe + Rust WASM
  ├─ MediaPipe FaceLandmarker（面部关键点识别）
  └─ Rust WASM 计算器
     ├─ EARCalculator（眼睛开放比例）
     ├─ PERCLOSCalculator（眼睛闭合百分比，60 秒窗口）
     ├─ BlinkDetector（眨眼频率）
     ├─ YawnDetector（哈欠检测）
     ├─ HeadPoseEstimator（头部姿态估计）
     └─ FatigueScorer（综合疲劳分数 0-100）
```

**丧失原因**：
1. **摄像头访问绑定**：需要调用 `navigator.mediaDevices.getUserMedia()`，获取浏览器/设备摄像头权限
   - 位置：`frontend/src/lib/fatigue/CameraManager.ts`
   - 仅适用于：Web 浏览器、移动浏览器
   - 不适用于：native iOS/Android 应用（需重新实现 Camera API）

2. **MediaPipe WASM 模块依赖**：
   - 从 CDN 加载 `@mediapipe/tasks-vision`（面部关键点模型）
   - 完整模型体积较大（~8-15MB），需网络加载
   - 位置：`frontend/src/lib/constants.ts` 定义的 CDN URLs
   - 后端**没有对应的面部识别 API 端点**

3. **Rust WASM 计算引擎**：
   - 位置：`crates/visual-fatigue-wasm/` 
   - 包含 6 个计算器：EARCalculator、PERCLOSCalculator 等
   - **完全客户端侧计算**，后端无相应实现
   - 算法参数硬编码在前端：
     - EAR 阈值：0.2，平滑窗口：3
     - PERCLOS 阈值：0.2，窗口：60 秒
     - 头部下垂阈值：pitch 15°，roll 20°

4. **独立 Web Worker 线程**：
   - 位置：`frontend/src/workers/fatigue.worker.ts`
   - 运行 MediaPipe + WASM 计算（避免阻塞主线程）
   - 每 100ms 处理一帧视频
   - 每 5 秒上报一次疲劳分数到后端

**后端的角色**（非常有限）：
```
POST /api/amas/visual-fatigue
{
  "score": 45  // 仅接收前端计算的分数
}
```
- 位置：`src/routes/admin/amas.rs` 中的 `report_visual_fatigue()` 
- 功能：将疲劳分数存储到 `user_state.fatigue`
- 作用：疲劳分数影响 AMAS 算法的难度调整（`fatigue_difficulty_drop`）

**无法在其他客户端实现的核心困境**：
- iOS 需要重写 Camera 权限请求与视频处理
- Android 需要重写 MediaPipe 集成（可通过 MediaPipe Android SDK）
- 在 API 层面无法复现"实时面部检测"功能，只能上报预计算的分数

---

### 1.2 前端本地队列管理（实际上可以通过 API 复现）

**位置**：`frontend/src/lib/WordQueueManager.ts`

**表面上的"客户端逻辑"**：
- `pickNext()`: 选词算法
  - 优先选有错误的词（按最久未展示排序）
  - 然后按后端下发的 priority（AMAS 排序）选择
- `generateOptions()`: 生成选项（4 个选项，1 正确 + 3 干扰项）
- `recordAnswer()`: 记录答题结果
- `computeSessionMetrics()`: 计算会话指标

**结论**：
- ✅ 这是 **UI 层的选词优化**，不是学习算法
- ✅ 所有关键决策（priority、difficulty、batch_size）来自后端 AMAS
- ✅ 其他客户端完全可以实现类似的 UI 选词逻辑
- ✅ 无需后端支持额外 API

---

## 2. 完整可用的后端 API（其他客户端可用）

### 2.1 学习相关 API

| 端点 | 功能 | 状态 |
|-----|------|------|
| `POST /api/learning/session` | 创建/恢复学习会话 | ✅ 完全服务端 |
| `GET /api/learning/study-words` | 获取初始学习词表 | ✅ 完全服务端 |
| `POST /api/learning/next-words` | 获取下一批词 | ✅ 完全服务端 |
| `POST /api/learning/adjust-words` | 调整难度时获取词 | ✅ 完全服务端 |
| `POST /api/learning/sync-progress` | 批量同步答题记录 | ✅ 完全服务端 |
| `POST /api/learning/complete-session` | 完成学习会话 | ✅ 完全服务端 |

### 2.2 AMAS 学习引擎 API

| 端点 | 功能 | 状态 |
|-----|------|------|
| `GET /api/amas/state` | 查询用户学习状态 | ✅ 完全服务端 |
| `GET /api/amas/strategy` | 获取当前策略（难度、batch_size） | ✅ 完全服务端 |
| `GET /api/amas/phase` | 获取当前学习阶段 | ✅ 完全服务端 |
| `GET /api/amas/learning-curve` | 学习曲线数据 | ✅ 完全服务端 |
| `GET /api/amas/intervention` | 获取干预建议 | ✅ 完全服务端 |
| `POST /api/amas/process-event` | 处理单个答题事件 | ✅ 完全服务端 |
| `POST /api/amas/batch-process` | 批量处理事件 | ✅ 完全服务端 |
| `POST /api/amas/visual-fatigue` | 上报疲劳分数 | ✅ 完全服务端 |

### 2.3 单词管理 API

| 端点 | 功能 | 状态 |
|-----|------|------|
| `GET /api/words` | 列出单词 | ✅ 完全服务端 |
| `POST /api/words` | 创建单词 | ✅ 完全服务端 |
| `POST /api/words/batch` | 批量创建 | ✅ 完全服务端 |
| `GET /api/words/:id` | 获取单词详情 | ✅ 完全服务端 |
| `PUT /api/words/:id` | 更新单词 | ✅ 完全服务端 |
| `DELETE /api/words/:id` | 删除单词 | ✅ 完全服务端 |

### 2.4 单词状态 & 学习进度 API

| 端点 | 功能 | 状态 |
|-----|------|------|
| `GET /api/word-states/batch` | 批量获取单词学习状态 | ✅ 完全服务端 |
| `POST /api/word-states/batch-update` | 批量更新状态 | ✅ 完全服务端 |
| `GET /api/records` | 学习记录 | ✅ 完全服务端 |
| `GET /api/word-states/due/list` | 获取待复习单词 | ✅ 完全服务端 |

---

## 3. 前端本地存储的数据（可以转移到后端）

前端使用 `localStorage` 和 `sessionStorage` 存储的数据：

| 数据 | 位置 | 后端支持 | 迁移方案 |
|-----|------|--------|--------|
| 学习模式（word-to-meaning 或 meaning-to-word） | `LEARNING_MODE` | ✅ 支持 (study_config API) | API 同步 |
| 当前会话 ID | `LEARNING_SESSION_ID` | ✅ 支持 | API 创建会话 |
| 学习队列（active/mastered 词） | `LEARNING_QUEUE` | ⚠️ 部分支持 | 从后端重新构建 |
| 疲劳检测启用状态 | `FATIGUE_ENABLED` | ❌ 无对应 API | 需要新增 API |
| 认证 Token | 内存 + sessionStorage | ✅ 支持 | 现有 auth API |

**结论**：
- ✅ 学习记录完全由后端管理
- ⚠️ 本地队列缓存可以从后端 API 重新加载
- ❌ 疲劳检测开关状态无后端支持（但可以添加）

---

## 4. 其他客户端的实现建议

### 4.1 iOS 原生应用（Swift）

**可以实现的功能**：
- ✅ 所有学习功能（通过 REST API）
- ✅ AMAS 引擎交互（算法运行在服务端）
- ✅ 单词管理 CRUD
- ❌ 实时视觉疲劳检测（需要 Vision Framework + 摄像头权限）

**实现难度**：
- 学习引擎：低（纯 API 调用）
- 疲劳检测：**高**（需要 Vision Framework + 人脸识别模型）

### 4.2 Android 原生应用（Kotlin）

**可以实现的功能**：
- ✅ 所有学习功能（通过 REST API）
- ⚠️ 疲劳检测（可通过 MediaPipe Android SDK）

### 4.3 Vue/React Web 应用

**可以实现的功能**：
- ✅ 所有学习功能
- ✅ 疲劳检测（使用相同的 MediaPipe + WASM）

---

## 5. 后端缺陷与改进建议

### 5.1 需要新增的 API 端点

| 需求 | 优先级 | 说明 |
|------|--------|------|
| 保存/获取疲劳检测启用状态 | 中 | 用户偏好管理 |
| 批量获取 word_id → priority 映射 | 低 | 优化队列预取 |

### 5.2 文档问题

- [ ] 没有明确说明疲劳检测是**客户端侧算法**
- [ ] 没有详细说明 MediaPipe WASM 的依赖和加载流程
- [ ] 算法参数硬编码在代码中，难以外部配置

---

## 6. 最终结论

### 6.1 丧失的功能（绝对）

| 功能 | 丧失原因 | 影响范围 |
|------|--------|--------|
| **实时视觉疲劳检测** | MediaPipe WASM 模块完全客户端侧；无后端面部识别 API | iOS/Android 需要重新实现 |

### 6.2 可以通过 API 完全复现的功能

- ✅ 学习流程与会话管理
- ✅ AMAS 自适应算法（完全在后端）
- ✅ 单词 CRUD 与进度跟踪
- ✅ 学习统计与曲线分析
- ✅ 离线答题与批量同步
- ✅ 用户认证与授权

### 6.3 建议

1. **短期**：为其他客户端提供疲劳检测的备选方案（上报简单指标而非实时检测）
2. **中期**：考虑在后端实现简化的疲劳检测 API（接收客户端上报的疲劳信号）
3. **长期**：评估在后端部署人脸识别服务（如 AWS Rekognition），为所有客户端提供统一的疲劳检测

---

**分析员**：Coupling Analyst  
**验证方式**：代码审计 + API 端点清查  
**置信度**：高（100%）
