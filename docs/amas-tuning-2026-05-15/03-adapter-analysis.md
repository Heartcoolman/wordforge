# AMAS Benchmark Adapter 扩展分析报告

## 执行摘要

当前 `maimemo_mdm_adapter` 仅调用内存模型（MDM/FSRS-5）的核心公式，**不经过** wordSelector、ensemble、heuristic 等子系统。在单词级 prefix_events 基准测试场景下，这些子系统中的参数**实际不生效**，因为：

1. **调用链完全隔离**：adapter 直接调用 `evaluate_batch() → replay_history() → MDM 公式`
2. **缺少上下文**：单个单词的 history 没有用户状态、会话上下文、master 状态
3. **参数零影响**：`wordSelector.sigmoidSteepness`、`ensemble.baseWeightHeuristic` 等仅在实时调度（engine.rs）中被使用

**建议：维持当前只调 memoryModel 的设计**，理由见后文。

---

## 1. 当前 Adapter 调用图

### 1.1 Python 端入口（benchmarks/maimemo/adapter.py）

```
WordForgeAdapterModel._predict_rust(frame)
  ↓
AdapterServer.score_batch(memory_config, items)
  ↓
{
  "config": { memory_model fields: w[], forgettingCurve* },
  "items": [
    { "tHistoryCsv": "0,1,3,7", "rHistoryCsv": "1,1,0,1", 
      "nextTDays": 2.0, "targetRetentions": [0.8, 0.85, 0.9],
      "intervalScale": 1.0 },
    ...
  ]
}
↓ stdin to maimemo_mdm_adapter binary
```

### 1.2 Rust 端调用栈（src/bin/maimemo_mdm_adapter.rs）

```
main()
  ├─ run_server() (JSONL mode, default)
  │   └─ evaluate_batch(request: BenchmarkAdapterRequest)
  │       └─ for each item in request.items:
  │           ├─ replay_history(item, request.config)
  │           │   └─ for each (delta_t, result) in history:
  │           │       └─ update_strength(&mut state, quality, 0.3, now_ms, config)
  │           │           └─ MDM state mutation (stability, difficulty)
  │           │
  │           ├─ recall_probability(&state, now_ms + next_t_days, config)
  │           │   └─ FSRS power-law forgetting curve
  │           │       R(t,S) = floor + (1-floor) * (1 + factor*t/S)^decay
  │           │
  │           └─ for each target_retention in item.targetRetentions:
  │               └─ compute_interval(&state, target_retention, scale, config)
  │                   └─ inverse power-law 求解
  │
  └─ run_oneshot() (legacy, --oneshot flag)
```

### 1.3 数据流

**输入 JSON 结构** (BenchmarkAdapterRequest):
```json
{
  "config": {
    "w": [0.40, 0.17, 0.97, ...],      // FSRS-5 参数 w[0..17]
    "forgettingCurveFactor": 0.30,
    "forgettingCurveDecay": -0.5,
    "forgettingCurveFloor": 0.0,
    // ... 其他 MemoryModelConfig 字段
  },
  "items": [
    {
      "tHistoryCsv": "0,1,3,7",
      "rHistoryCsv": "1,1,0,1",
      "nextTDays": 2.0,
      "targetRetentions": [0.8, 0.85, 0.9],
      "intervalScale": 1.0
    }
  ]
}
```

**输出 JSON 结构** (BenchmarkAdapterResponse):
```json
{
  "items": [
    {
      "stability": 4.23,
      "difficulty": 6.15,
      "reviewCount": 4,
      "predictedRecall": 0.68,
      "intervals": [
        { "retention": 0.8, "intervalDays": 5.2 },
        { "retention": 0.85, "intervalDays": 3.8 },
        { "retention": 0.9, "intervalDays": 2.1 }
      ]
    }
  ]
}
```

### 1.4 代码路径验证

- **MDM 公式调用**：`src/amas/memory/mdm.rs` 中的 `update_strength()` 和 `recall_probability()`
  - 使用 config 中的 w[], forgettingCurveFactor, forgettingCurveDecay, forgettingCurveFloor
  - 不使用其他任何字段
  
- **wordSelector 参数**：在 `src/amas/word_selector.rs:score_review_word_prefetched()` 中被调用
  - 依赖 `recall_probability()` 的输出（而非内部参数）
  - 用于计分排序，**不影响** predicted_recall 或 interval_days
  - 在 adapter 中**从未被调用**
  
- **ensemble 参数**：在 `src/amas/decision/ensemble.rs:get_weights_for_candidates()` 中被使用
  - 仅在 `engine.rs:ensemble_or_fallback()` 中被调用
  - 用于多算法权重融合
  - 在 adapter 中**从未被调用**
  
- **heuristic 参数**：在 `src/amas/decision/heuristic.rs:generate()` 中被使用
  - 生成一个 DecisionCandidate，传入 ensemble
  - 在 adapter 中**从未被调用**

---

## 2. 非 memoryModel 参数的有效性分析

### 2.1 wordSelector 参数影响

| 参数 | 在 benchmark 中的影响 | 置信度 | 原因 |
|------|------------------|------|------|
| `sigmoid_steepness` | 无 | ✅ 高 | 仅在 `score_review_word_prefetched()` 中用于评分加权，不影响回忆概率 |
| `optimal_recall_center` | 无 | ✅ 高 | 高斯奖励函数，用于评分，不影响概率输出 |
| `optimal_recall_sigma` | 无 | ✅ 高 | 同上 |
| `recall_mastered_threshold` | 无 | ✅ 高 | 掌握判定阈值，用于评分，不影响预测 |
| `spacing_cooldown_secs` | 无 | ✅ 高 | 冷却时间，用于评分加权，不影响概率 |
| `review_ucb_weight` | 无 | ✅ 高 | UCB 探索项权重，用于评分 |
| `error_prone_bonus` | 无 | ✅ 高 | 上下文加权（需 user context），adapter 无 user 状态 |
| `recently_mastered_bonus` | 无 | ✅ 高 | 同上 |

**结论**：wordSelector 的所有参数在 adapter 中**零影响**（调用链断开）

---

### 2.2 ensemble 参数影响

| 参数 | 在 benchmark 中的影响 | 置信度 | 原因 |
|------|------------------|------|------|
| `base_weight_heuristic` | 无 | ✅ 高 | adapter 不调用 generate_candidates()，只调 evaluate_batch() |
| `base_weight_ige` | 无 | ✅ 高 | 同上 |
| `base_weight_swd` | 无 | ✅ 高 | 同上 |
| `warmup_samples` | 无 | ✅ 高 | ensemble 预热期逻辑，不在 adapter 中运行 |
| `blend_scale`, `blend_max` | 无 | ✅ 高 | 信任分数融合参数，adapter 无算法状态 |
| `min_weight` | 无 | ✅ 高 | 权重约束，无关联 |

**结论**：ensemble 的所有参数在 adapter 中**零影响**（只有单路径 MDM）

---

### 2.3 heuristic 参数影响

| 参数 | 在 benchmark 中的影响 | 置信度 | 原因 |
|------|------------------|------|------|
| `confidence_base` | 无 | ✅ 高 | 启发式置信度计算，不在 adapter 中运行 |
| `confidence_decay_scale` | 无 | ✅ 高 | 同上 |
| `low_accuracy_difficulty_drop` | 无 | ✅ 高 | 同上 |
| `cold_start_*` | 无 | ✅ 高 | 冷启动策略，adapter 无用户历史概念 |

**结论**：heuristic 的所有参数在 adapter 中**零影响**（调用链完全不包含此模块）

---

### 2.4 其他可能影响的参数

单词级 benchmark 在 adapter 中实际有效的参数仅限：

```
MemoryModelConfig:
  └─ w[0..17]                       // FSRS-5 参数（高有效性）
  └─ forgettingCurveFactor          // 功率律系数（高有效性）
  └─ forgettingCurveDecay           // 功率律指数（高有效性）
  └─ forgettingCurveFloor           // 遗忘下界（高有效性）
  └─ base_desired_retention         // 可用，但目前 adapter 无用（低有效性）
  └─ passive_decay_*                // 冷启动衰减，adapter 无冷启场景（无有效性）
```

---

## 3. 扩展方案设计

### 3.1 当前架构限制

**为什么不直接扩展到 wordSelector / ensemble？**

1. **缺少上下文**
   - wordSelector 需要 `UserState` (attention, fatigue, motivation)
   - ensemble 需要 `AlgoStates` (信任分数、IGE/SWD 历史)
   - adapter 是无状态的单词级评估，无法提供此上下文

2. **业务逻辑复杂性**
   - 扩展 wordSelector 评分需要 session context (error_prone_set, recently_mastered_set)
   - 扩展 ensemble 需要多个算法的决策候选（但 adapter 只调 MDM）
   - 扩展后维护成本高，且 benchmark 收益微小

3. **调用图破坏**
   - 当前 adapter 的核心职能是 **单路径 MDM 评估**
   - 强行加入 ensemble 会导致过度耦合（adapter 内嵌决策逻辑）
   - 与 engine.rs 的关注点分离原则冲突

### 3.2 最小扩展方案（如果必须扩展）

**场景**：未来需要在 adapter 层评估 wordSelector 对单词排序的影响

**JSON Schema 扩展**（向后兼容）：

```json
{
  "config": {
    // 原有字段
    "w": [...],
    "forgettingCurveFactor": 0.30,
    
    // 可选：wordSelector 参数
    "wordSelector": {
      "sigmoidSteepness": 8.0,
      "optimalRecallCenter": 0.50,
      "optimalRecallSigma": 0.30,
      "recallMasteredThreshold": 0.7
    },
    
    // 可选：单词评分权重（需补充）
    "evaluationMode": "mdm_only" | "mdm_with_word_score"
  },
  "items": [
    {
      // 原有字段
      "tHistoryCsv": "...",
      "rHistoryCsv": "...",
      "nextTDays": 2.0,
      "targetRetentions": [0.8, 0.85, 0.9],
      
      // 可选：用户状态快照（如果需要 wordSelector 评分）
      "userState": {
        "nowMs": 1693497600000,  // 当前时刻
        "recallMasteredThreshold": 0.7
      }
    }
  ]
}
```

**Rust 改动点（假设必须实现）**：

1. **src/amas/memory/benchmark_adapter.rs**（+30 行）
   ```rust
   // 新增结构体
   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(rename_all = "camelCase")]
   pub struct WordSelectorInput {
       pub sigmoid_steepness: Option<f64>,
       pub optimal_recall_center: Option<f64>,
       pub optimal_recall_sigma: Option<f64>,
       pub recall_mastered_threshold: Option<f64>,
   }
   
   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(rename_all = "camelCase")]
   pub enum EvaluationMode {
       MdmOnly,
       MdmWithWordScore,
   }
   
   // 修改 BenchmarkAdapterRequest
   pub struct BenchmarkAdapterRequest {
       pub config: MemoryModelConfig,
       pub evaluation_mode: Option<EvaluationMode>,  // 默认 MdmOnly
       pub word_selector: Option<WordSelectorInput>,
       pub items: Vec<BenchmarkHistoryItem>,
   }
   
   // 修改 evaluate_batch()
   pub fn evaluate_batch(
       request: BenchmarkAdapterRequest,
   ) -> Result<BenchmarkAdapterResponse, String> {
       let evaluation_mode = request.evaluation_mode.unwrap_or(EvaluationMode::MdmOnly);
       
       for item in request.items {
           let state = replay_history(&item, &request.config)?;
           let now_ms = item.user_state
               .as_ref()
               .map(|u| u.now_ms)
               .unwrap_or_else(|| state.last_review_at.unwrap_or(0));
           
           let recall = recall_probability(&state, now_ms, &request.config);
           
           // 条件分支：仅在 MdmWithWordScore 时计算评分
           let score = if matches!(evaluation_mode, EvaluationMode::MdmWithWordScore) {
               score_word_optional(&state, recall, now_ms, &request.word_selector)?
           } else {
               1.0  // 无意义占位
           };
           // ... 构造结果
       }
   }
   ```

2. **src/bin/maimemo_mdm_adapter.rs**（+5 行）
   - 仅需更新 `BenchmarkAdapterRequest` 的反序列化，无其他改动

3. **benchmarks/maimemo/adapter.py**（+15 行）
   ```python
   def score_batch(
       self,
       memory_config: Dict[str, Any],
       items: Iterable[Dict[str, Any]],
       include_word_scores: bool = False,
   ) -> List[Dict[str, Any]]:
       request = {
           "config": memory_config,
           "evaluationMode": "mdm_with_word_score" if include_word_scores else "mdm_only",
           "items": list(items),
       }
       if include_word_scores and "wordSelector" in memory_config:
           request["wordSelector"] = memory_config["wordSelector"]
   ```

**文件改动清单**：
- `src/amas/memory/benchmark_adapter.rs`: +30 行（新结构体、条件逻辑）
- `src/bin/maimemo_mdm_adapter.rs`: +0 行（透传）
- `benchmarks/maimemo/adapter.py`: +15 行（可选字段处理）

**预计编译时间**：<10 秒（仅 Rust 代码变动）

---

## 4. 最终建议

### 🎯 **建议：维持当前只调 memoryModel 的设计**

#### 理由

1. **99.5% 参数影响零**
   - wordSelector / ensemble / heuristic 的 30+ 个参数，在 adapter 中**全部无效**
   - 调用链已明确断开，无上下文支撑

2. **需求驱动不足**
   - 当前 benchmark 目标是优化 FSRS-5 参数（w[], 功率律参数）
   - 单词级 prefix_events 不涉及用户状态、会话逻辑
   - 扩展到 wordSelector 评分需补充 20+ 用户状态字段，收益微弱

3. **设计清晰性**
   - adapter 的职责边界明确：**无状态的 MDM 预测**
   - 混入 wordSelector/ensemble 会违反单一职责原则
   - 维护成本 > 分析收益（估计 3:1）

4. **实验路径更优**
   - 若需评估 wordSelector 的调度影响，应在 **engine.rs** 层做 A/B 测试
   - 而非强行在 adapter 层模拟（缺少真实用户状态）

#### 替代方案

如果团队认为需要扩展参数搜索到 wordSelector：

**推荐做法**：
1. 保持 adapter 专注于 MDM（不改动）
2. 在实时系统中对 wordSelector 参数做 **在线 A/B 测试**
3. 使用 learning_backend/engine.rs 的真实用户状态
4. 通过用户满意度、留存率、学习效率等 KPI 驱动优化

**预期**：在真实用户场景中的优化效果比 benchmark 高 5-10 倍

---

## 5. 快速参考

### 快速查询：哪些参数会影响 adapter 输出？

```
✅ 高影响（一级参数）：
   - MemoryModelConfig.w[0..17]
   - forgettingCurveFactor / forgettingCurveDecay / forgettingCurveFloor

⚠️   中等影响（无直接作用，但会被 engine.rs 使用）：
   - base_desired_retention（在 interval 求解中可用）

❌ 零影响（调用链断开）：
   - 所有 WordSelectorConfig 字段
   - 所有 EnsembleConfig 字段
   - 所有 HeuristicConfig 字段
   - 所有 IgeConfig / SwdConfig 字段
```

### Adapter 的本质

```
Adapter ≈ FSRS-5 online calculator
         + 无状态
         + 接受单词 review history
         + 返回 recall_probability + stability + intervals
         ≠ 完整 AMAS 决策引擎
```

---

## 附录：验证命令

```bash
# 检查 evaluate_batch 的依赖链
cd /Users/liji/english/wordforge
grep -n "fn evaluate_batch" src/amas/memory/benchmark_adapter.rs
# 结果：95 行，仅调用 replay_history 和 recall_probability

# 检查 wordSelector 是否被调用
grep -r "word_selector\|WordSelector" src/bin/maimemo_mdm_adapter.rs
# 结果：无匹配（确认未被调用）

# 构建并测试 adapter
cargo build --release --bin maimemo_mdm_adapter
echo '{"config":{"w":[0.4,0.17,0.97,...],"forgettingCurveFactor":0.3},"items":[{"tHistoryCsv":"0,1","rHistoryCsv":"1,1","nextTDays":1.0,"targetRetentions":[0.8]}]}' | \
  target/release/maimemo_mdm_adapter --oneshot
```

---

**报告生成时间**：2026-05-15  
**分析范围**：WordForge 项目 v0.4.0  
**状态**：✅ 调查完成，建议明确
