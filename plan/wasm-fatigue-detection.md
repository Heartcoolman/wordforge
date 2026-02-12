# WASM 视觉疲劳检测 — 集成方案

## 一、架构总览

```
┌─────────────────────────────────────────────────────────────────┐
│                        主线程 (SolidJS)                         │
│  ┌──────────┐  ┌──────────────┐  ┌───────────────────────────┐ │
│  │摄像头管理 │  │fatigueStore  │  │ UI 组件                    │ │
│  │Camera     │→ │ createRoot   │→ │ FatigueIndicator          │ │
│  │Manager    │  │ 单例 store   │  │ FatigueWarningModal       │ │
│  └─────┬─────┘  └──────▲───────┘  │ CameraPermissionDialog   │ │
│        │               │          └───────────────────────────┘ │
│        │ ImageBitmap    │ postMessage(结果)                      │
│        ▼               │                                        │
│  ┌─────────────────────┴────────────────────────────┐          │
│  │              Web Worker                           │          │
│  │  ┌─────────────────┐  ┌────────────────────────┐ │          │
│  │  │ MediaPipe        │  │ Rust WASM 算法模块      │ │          │
│  │  │ FaceLandmarker   │→ │ EAR → PERCLOS → 眨眼   │ │          │
│  │  │ (478 关键点)      │  │ MAR → 哈欠检测         │ │          │
│  │  │ (Blendshapes)    │  │ 头部姿态 → 综合评分     │ │          │
│  │  └─────────────────┘  └────────────────────────┘ │          │
│  └──────────────────────────────────────────────────┘          │
└─────────────────────────────────────────────────────────────────┘
```

**核心设计原则：**
- MediaPipe 做面部关键点检测（利用 WebGL 加速），Rust WASM 做纯数值疲劳算法
- 所有推理和计算在 Web Worker 中运行，不阻塞主线程
- 视频数据不离开浏览器，仅上报疲劳数值（隐私优先）
- 用户主动开启，随时可关闭

---

## 二、目录结构

### 2.1 新增 Rust WASM Crate

```
english/
├── crates/
│   └── visual-fatigue-wasm/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs              # 入口，导出所有模块
│           ├── ear.rs              # EAR (Eye Aspect Ratio) 眨眼比率
│           ├── perclos.rs          # PERCLOS 闭眼时间百分比
│           ├── blink.rs            # 眨眼检测状态机
│           ├── yawn.rs             # 哈欠检测 (MAR)
│           ├── head_pose.rs        # 头部姿态估计
│           └── fatigue.rs          # 综合疲劳评分（加权融合）
```

### 2.2 新增前端文件

```
frontend/src/
├── workers/
│   └── fatigue.worker.ts           # Web Worker：MediaPipe + WASM 桥接
├── lib/
│   └── fatigue/
│       ├── CameraManager.ts        # 摄像头生命周期管理
│       └── index.ts                # WASM 初始化封装
├── stores/
│   └── fatigue.ts                  # 疲劳检测状态 store (createRoot 单例)
├── hooks/
│   └── useFatigueDetection.ts      # 页面级 hook（摄像头 + Worker 控制）
├── components/
│   └── fatigue/
│       ├── FatigueToggle.tsx        # 开关按钮（Header 区域）
│       ├── FatigueIndicator.tsx     # 疲劳等级圆环指示器
│       ├── FatigueWarningModal.tsx  # 休息建议弹窗
│       └── CameraPermission.tsx    # 摄像头权限引导
```

---

## 三、核心模块设计

### 3.1 Rust WASM Crate (`crates/visual-fatigue-wasm`)

#### Cargo.toml
```toml
[package]
name = "visual-fatigue-wasm"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2"
js-sys = "0.3"
serde = { version = "1.0", features = ["derive"] }
serde-wasm-bindgen = "0.6"

[profile.release]
opt-level = "s"
lto = true
```

#### 算法模块概述

| 模块 | 输入 | 输出 | 算法 |
|------|------|------|------|
| `ear.rs` | 眼部关键点坐标 (Float64Array) | EAR 值 + 置信度 | 6点/16点 EAR 公式 |
| `perclos.rs` | EAR 值序列 | PERCLOS 百分比 | 60秒滑动窗口 |
| `blink.rs` | EAR 值 + 时间戳 | 眨眼事件 + 频率 | 四状态机: Open→Closing→Closed→Opening |
| `yawn.rs` | 嘴部关键点坐标 | 哈欠事件 + 频率 | MAR 阈值 0.6 + 持续时间 2-8秒 |
| `head_pose.rs` | 面部变换矩阵/关键点 | pitch/yaw/roll | 欧拉角提取 + 下垂检测 |
| `fatigue.rs` | 以上所有指标 | 0-100 综合疲劳分 | 五维加权: PERCLOS 30% + 眨眼 20% + 哈欠 20% + 头部 15% + 表情 15% |

#### 疲劳等级定义

| 等级 | 分数范围 | 颜色 | 行为 |
|------|---------|------|------|
| 清醒 (Alert) | 0-25 | 绿色 | 正常学习 |
| 轻度疲劳 (Mild) | 25-50 | 黄色 | Toast 提示"适当休息" |
| 中度疲劳 (Moderate) | 50-75 | 橙色 | 警告提示 + 指示器变色 |
| 严重疲劳 (Severe) | 75-100 | 红色 | Modal 弹窗建议停止学习 |

### 3.2 Web Worker (`fatigue.worker.ts`)

**职责：**
1. 初始化 MediaPipe FaceLandmarker（加载模型）
2. 初始化 Rust WASM 算法模块（7 个计算实例）
3. 接收主线程的 ImageBitmap，提取 478 关键点
4. 将关键坐标送入 WASM 计算 EAR/MAR/PERCLOS/头部姿态
5. 返回综合疲劳评分给主线程

**消息协议：**
```typescript
// 主线程 → Worker
type WorkerCommand =
  | { type: 'init' }                      // 初始化
  | { type: 'process'; bitmap: ImageBitmap } // 处理帧
  | { type: 'reset' }                     // 重置状态
  | { type: 'destroy' }                   // 销毁

// Worker → 主线程
type WorkerResult =
  | { type: 'ready' }                     // 初始化完成
  | { type: 'result'; data: FatigueResult } // 检测结果
  | { type: 'error'; message: string }    // 错误
```

**检测帧率：** 100ms 间隔 (~10 FPS)，足以满足疲劳检测精度

### 3.3 前端 Store (`stores/fatigue.ts`)

```typescript
// 遵循项目现有的 createRoot + createSignal 模式
function createFatigueStore() {
  const [enabled, setEnabled] = createSignal(false);        // 功能开关
  const [detecting, setDetecting] = createSignal(false);    // 正在检测
  const [wasmReady, setWasmReady] = createSignal(false);    // WASM 已加载
  const [cameraReady, setCameraReady] = createSignal(false);// 摄像头已授权
  const [fatigueScore, setFatigueScore] = createSignal(0);  // 0-100
  const [fatigueLevel, setFatigueLevel] = createSignal<     // 等级
    'alert' | 'mild' | 'moderate' | 'severe'
  >('alert');
  const [blinkRate, setBlinkRate] = createSignal(0);        // 眨眼率/分钟
  const [perclos, setPerclos] = createSignal(0);            // PERCLOS %
  const [sessionDuration, setSessionDuration] = createSignal(0); // 本次检测时长

  // 持久化用户偏好到 localStorage
  // enabled 状态记忆用户上次选择
}

export const fatigueStore = createRoot(createFatigueStore);
```

### 3.4 页面集成 Hook (`hooks/useFatigueDetection.ts`)

```typescript
export function useFatigueDetection() {
  // 在 LearningPage / FlashcardPage 中调用
  // onMount: 如果 enabled，自动启动摄像头 + Worker
  // onCleanup: 释放摄像头 + 终止 Worker
  // 返回: { start, stop, fatigueScore, fatigueLevel }
}
```

### 3.5 集成到学习页面的位置

**LearningPage.tsx：**
- Header 右侧添加 `<FatigueToggle />`（眼睛图标开关）
- 开启后在 Header 显示 `<FatigueIndicator />`（小圆环）
- 答题间隔（feedback phase 的 1-2 秒等待期）检查疲劳等级
- 严重疲劳时在答题结束后弹出 `<FatigueWarningModal />`

**FlashcardPage.tsx：**
- 同样在 Header 集成开关和指示器
- 翻牌间隙检查疲劳等级

---

## 四、数据流详解

```
用户点击"开启疲劳检测"
    ↓
请求摄像头权限 (getUserMedia, 640x480, 15fps)
    ↓
启动 Web Worker
    ↓
Worker 加载 MediaPipe 模型 (~2-3MB, 首次)
Worker 加载 WASM 模块 (~50-100KB)
    ↓
主线程每 100ms: createImageBitmap(video) → postMessage(bitmap, [bitmap])
    ↓
Worker: FaceLandmarker.detect(bitmap)
    → 478 关键点 + Blendshapes + 变换矩阵
    ↓
Worker: 提取眼部/嘴部关键坐标 → Float64Array
    → WASM EARCalculator.calculate()
    → WASM PERCLOSCalculator.update(ear)
    → WASM BlinkDetector.update(ear, timestamp)
    → WASM YawnDetector.update(mar, timestamp)
    → WASM HeadPoseEstimator.update(matrix)
    → WASM FatigueScorer.calculate(perclos, blink, yawn, head, expression)
    ↓
Worker → postMessage({ type: 'result', data: { score, level, ... } })
    ↓
主线程: fatigueStore 更新信号 → UI 响应式更新
    ↓
score < 25  → 绿色指示器，无提示
score 25-50 → 黄色指示器，Toast 提示
score 50-75 → 橙色指示器，警告
score > 75  → 红色指示器，Modal 弹窗建议休息
```

---

## 五、隐私保护设计

1. **用户主动授权**：点击按钮 → 显示说明弹窗 → 确认后请求摄像头
2. **本地处理**：所有视频帧在浏览器 Worker 中处理，绝不上传服务器
3. **不存储视频**：处理完立即丢弃帧数据，仅保留数值指标
4. **可见状态**：摄像头激活时显示绿色圆点指示器
5. **一键关闭**：随时停止检测并释放摄像头
6. **偏好记忆**：记住用户选择（localStorage），下次不重复询问

---

## 六、实施步骤（按优先级分阶段）

### 阶段一：Rust WASM 核心算法（P0）
1. 创建 `crates/visual-fatigue-wasm/` crate
2. 实现 `ear.rs` — EAR 计算（6点 + 增强16点）
3. 实现 `perclos.rs` — PERCLOS 滑动窗口
4. 实现 `blink.rs` — 眨眼检测状态机
5. 实现 `yawn.rs` — 哈欠检测 (MAR)
6. 实现 `head_pose.rs` — 头部姿态估计
7. 实现 `fatigue.rs` — 五维加权综合评分
8. `wasm-pack build --target web` 编译验证

### 阶段二：Web Worker + MediaPipe 集成（P0）
1. 安装 `@mediapipe/tasks-vision`
2. 安装 `vite-plugin-wasm` + `vite-plugin-top-level-await`，更新 vite.config.ts
3. 编写 `fatigue.worker.ts`（MediaPipe 初始化 + WASM 调用 + 消息协议）
4. 编写 `CameraManager.ts`（摄像头生命周期管理）
5. 端到端验证：摄像头 → Worker → WASM → 疲劳分数

### 阶段三：前端 UI 集成（P1）
1. 创建 `fatigueStore`（createRoot 单例）
2. 编写 `useFatigueDetection` hook
3. 实现 `FatigueToggle` 开关组件
4. 实现 `FatigueIndicator` 圆环指示器
5. 实现 `CameraPermission` 权限引导弹窗
6. 实现 `FatigueWarningModal` 休息建议弹窗
7. 集成到 LearningPage 和 FlashcardPage

### 阶段四：优化与测试（P2）
1. WASM 体积优化 (wasm-opt, LTO)
2. MediaPipe 模型预加载/缓存策略
3. 低端设备降级策略（降帧率/降分辨率）
4. 单元测试（WASM 算法 + Store + 组件）
5. 疲劳提醒的用户体验调优

---

## 七、关键依赖清单

| 依赖 | 位置 | 用途 |
|------|------|------|
| `wasm-bindgen` | Rust crate | Rust ↔ JS 绑定 |
| `js-sys` | Rust crate | JS 类型访问 |
| `serde` + `serde-wasm-bindgen` | Rust crate | 结构体序列化 |
| `wasm-pack` | CLI 工具 | 编译 Rust → WASM |
| `@mediapipe/tasks-vision` | npm | 面部关键点检测 |
| `vite-plugin-wasm` | npm devDep | Vite WASM 支持 |
| `vite-plugin-top-level-await` | npm devDep | top-level await |

---

## 八、性能预算

| 指标 | 目标 |
|------|------|
| WASM 模块体积 | < 100KB (gzip) |
| MediaPipe 模型 | ~2-3MB (首次加载，后续缓存) |
| 每帧处理耗时 | < 50ms (Worker 线程) |
| 主线程影响 | < 2ms/帧 (仅 postMessage) |
| 内存占用 | < 50MB (含 MediaPipe 模型) |
| 检测帧率 | 10 FPS (100ms 间隔) |
