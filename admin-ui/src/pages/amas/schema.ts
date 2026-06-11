// AMAS 参数字典：UI 渲染 / 校验 / preset 的 single source of truth
//
// path 语义：
//   - "section.field"       → 普通嵌套字段
//   - "section.array[idx]"  → 数组下标（仅 memoryModel.w 用到）
//
// 字段元信息用于：
//   1. 重点参数页与分节页统一渲染（label / hint / 范围 / 步长）
//   2. 保存前预校验（避免来回 422）
//   3. preset 与 current 之间 diff 时给出可读字段名
//   4. LLM hover 解释（description_zh 作为兜底）

export type ParamType = 'number' | 'integer' | 'bool';
export type Sensitivity = 'ultra' | 'high' | 'med' | 'low';
export type Affects = 'retention' | 'prediction' | 'latency' | 'fatigue' | 'engagement' | 'exploration';

export interface ParamMeta {
  path: string;
  label_zh: string;
  type: ParamType;
  min?: number;
  max?: number;
  step?: number;
  default: number | boolean;
  description_zh: string;
  affects?: Affects[];
  sensitivity?: Sensitivity;
}

/** Tier-A 12 维：经调参验证收益最大的参数集合（FSRS-6 起含 w[20] 曲线 decay） */
export const TIER_A_PATHS: readonly string[] = [
  'memoryModel.w[0]',
  'memoryModel.w[1]',
  'memoryModel.w[2]',
  'memoryModel.w[3]',
  'memoryModel.w[8]',
  'memoryModel.w[9]',
  'memoryModel.w[10]',
  'memoryModel.w[15]',
  'memoryModel.w[16]',
  'memoryModel.baseDesiredRetention',
  'memoryModel.maxIntervalDays',
  'memoryModel.w[20]',
] as const;

/** 字段元数据；按 section 分组以便 UI 渲染 */
export const PARAM_DICT: Record<string, ParamMeta[]> = {
  featureFlags: [
    { path: 'featureFlags.ensembleEnabled', label_zh: '启用 3 路集成', type: 'bool', default: true, description_zh: 'Heuristic / IGE / SWD 三路集成投票，关闭后只用 Heuristic' },
    { path: 'featureFlags.heuristicEnabled', label_zh: '启用 Heuristic', type: 'bool', default: true, description_zh: '基于规则的冷启动策略' },
    { path: 'featureFlags.igeEnabled', label_zh: '启用 IGE', type: 'bool', default: true, description_zh: '信息增益探索（UCB bandit）' },
    { path: 'featureFlags.swdEnabled', label_zh: '启用 SWD', type: 'bool', default: true, description_zh: '相似度加权距离决策' },
    { path: 'featureFlags.mdmEnabled', label_zh: '启用 MDM 记忆模型', type: 'bool', default: true, description_zh: 'FSRS-6 难度-稳定性-可提取性模型；关闭后退回简单 SM-2' },
    { path: 'featureFlags.iadEnabled', label_zh: '启用 IAD', type: 'bool', default: false, description_zh: '干扰感知衰减（混淆词对追踪）；实验性' },
    { path: 'featureFlags.mtpEnabled', label_zh: '启用 MTP', type: 'bool', default: false, description_zh: '词素迁移预测；实验性' },
    { path: 'featureFlags.sspEnabled', label_zh: '启用 SSP-MMC', type: 'bool', default: false, description_zh: '离线 DP 最优调度（计算密集）；实验性' },
  ],
  ensemble: [
    { path: 'ensemble.baseWeightHeuristic', label_zh: 'Heuristic 初始权重', type: 'number', min: 0, max: 1, step: 0.01, default: 0.35, description_zh: '三路集成 Heuristic 通道的初始权重；三者之和需 ≈ 1', sensitivity: 'high' },
    { path: 'ensemble.baseWeightIge', label_zh: 'IGE 初始权重', type: 'number', min: 0, max: 1, step: 0.01, default: 0.4, description_zh: '三路集成 IGE 通道的初始权重；三者之和需 ≈ 1' },
    { path: 'ensemble.baseWeightSwd', label_zh: 'SWD 初始权重', type: 'number', min: 0, max: 1, step: 0.01, default: 0.25, description_zh: '三路集成 SWD 通道的初始权重；三者之和需 ≈ 1' },
    { path: 'ensemble.warmupSamples', label_zh: '热身样本数', type: 'integer', min: 1, max: 1000, step: 1, default: 20, description_zh: '热身阶段事件数；之后集成根据信任度动态调权' },
    { path: 'ensemble.blendScale', label_zh: '信任融合速率', type: 'number', min: 1, max: 1000, step: 1, default: 100, description_zh: '越大融合越慢，权重收敛越平滑' },
    { path: 'ensemble.blendMax', label_zh: '权重最大偏移', type: 'number', min: 0, max: 1, step: 0.01, default: 0.5, description_zh: '信任融合后偏离初始权重的上限' },
    { path: 'ensemble.minWeight', label_zh: '最小权重下限', type: 'number', min: 0, max: 0.33, step: 0.01, default: 0.15, description_zh: '任一通道的权重下限，防止某路被压成零' },
    { path: 'ensemble.warmupHeuristicBoost', label_zh: '热身期 Heuristic 加成', type: 'number', min: 0, max: 1, step: 0.01, default: 0.1, description_zh: '热身阶段额外赋予 Heuristic 的权重' },
  ],
  modeling: [
    { path: 'modeling.attentionSmoothing', label_zh: '注意力 EMA 系数', type: 'number', min: 0, max: 1, step: 0.01, default: 0.3, description_zh: '注意力信号的指数平滑系数（α）' },
    { path: 'modeling.confidenceDecay', label_zh: '信心衰减系数', type: 'number', min: 0, max: 1, step: 0.001, default: 0.99, description_zh: '每事件信心衰减；保留较多历史用 0.99 附近' },
    { path: 'modeling.minConfidence', label_zh: '信心下限', type: 'number', min: 0, max: 1, step: 0.01, default: 0.1, description_zh: '防止信心信号塌缩到零' },
    { path: 'modeling.fatigueIncreaseRate', label_zh: '疲劳累积速率', type: 'number', min: 0, max: 1, step: 0.001, default: 0.02, description_zh: '每个事件疲劳的累积量', affects: ['fatigue'], sensitivity: 'med' },
    { path: 'modeling.fatigueRecoveryRate', label_zh: '疲劳恢复速率', type: 'number', min: 0, max: 1, step: 0.0001, default: 0.001, description_zh: '休息时每秒疲劳衰减量', affects: ['fatigue'] },
    { path: 'modeling.motivationMomentum', label_zh: '动机动量', type: 'number', min: 0, max: 1, step: 0.01, default: 0.1, description_zh: '动机信号的 EMA 动量' },
    { path: 'modeling.visualFatigueWeight', label_zh: '视觉疲劳权重', type: 'number', min: 0, max: 1, step: 0.05, default: 0.4, description_zh: '视觉疲劳占总疲劳的比重（其余为行为疲劳）' },
  ],
  constraints: [
    { path: 'constraints.highFatigueThreshold', label_zh: '高疲劳阈值', type: 'number', min: 0, max: 1, step: 0.01, default: 0.9, description_zh: '触发疲劳保护约束的阈值', affects: ['fatigue'] },
    { path: 'constraints.lowAttentionThreshold', label_zh: '低注意力阈值', type: 'number', min: 0, max: 1, step: 0.01, default: 0.3, description_zh: '注意力低于此值时启用降难度策略' },
    { path: 'constraints.lowMotivationThreshold', label_zh: '低动机阈值', type: 'number', min: -1, max: 1, step: 0.05, default: -0.5, description_zh: '动机低于此值时启用激励策略' },
    { path: 'constraints.maxBatchSizeWhenFatigued', label_zh: '疲劳时批量上限', type: 'integer', min: 1, max: 50, step: 1, default: 5, description_zh: '疲劳状态下单次推送词数上限' },
    { path: 'constraints.maxNewRatioWhenFatigued', label_zh: '疲劳时新词比例上限', type: 'number', min: 0, max: 1, step: 0.05, default: 0.2, description_zh: '疲劳时新词占比上限' },
    { path: 'constraints.maxDifficultyWhenFatigued', label_zh: '疲劳时难度上限', type: 'number', min: 0, max: 1, step: 0.05, default: 0.55, description_zh: '疲劳时单次任务难度上限' },
    { path: 'constraints.minDifficulty', label_zh: '难度下限', type: 'number', min: 0, max: 1, step: 0.05, default: 0.1, description_zh: '所有场景下的难度绝对下限' },
  ],
  objectiveWeights: [
    { path: 'objectiveWeights.retention', label_zh: '留存权重', type: 'number', min: 0, max: 1, step: 0.05, default: 0.35, description_zh: '目标函数中留存的权重；五项和需 ≈ 1', affects: ['retention'] },
    { path: 'objectiveWeights.accuracy', label_zh: '准确率权重', type: 'number', min: 0, max: 1, step: 0.05, default: 0.25, description_zh: '目标函数中准确率的权重；五项和需 ≈ 1' },
    { path: 'objectiveWeights.speed', label_zh: '速度权重', type: 'number', min: 0, max: 1, step: 0.05, default: 0.15, description_zh: '目标函数中响应速度的权重；五项和需 ≈ 1' },
    { path: 'objectiveWeights.fatigue', label_zh: '疲劳权重', type: 'number', min: 0, max: 1, step: 0.05, default: 0.15, description_zh: '目标函数中疲劳抑制的权重；五项和需 ≈ 1' },
    { path: 'objectiveWeights.frustration', label_zh: '挫折权重', type: 'number', min: 0, max: 1, step: 0.05, default: 0.1, description_zh: '目标函数中挫折抑制的权重；五项和需 ≈ 1' },
  ],
  memoryModel: [
    // Tier-A —— FSRS-6 w 数组中已验证的高杠杆下标
    { path: 'memoryModel.w[0]', label_zh: '初始稳定性 · Again', type: 'number', min: 0.01, max: 5, step: 0.01, default: 0.212, description_zh: 'FSRS-6 w[0]：评级 Again 时的初始稳定性（天）', affects: ['retention', 'prediction'], sensitivity: 'high' },
    { path: 'memoryModel.w[1]', label_zh: '初始稳定性 · Hard', type: 'number', min: 0.01, max: 5, step: 0.01, default: 1.2931, description_zh: 'FSRS-6 w[1]：评级 Hard 时的初始稳定性（天）', affects: ['retention', 'prediction'], sensitivity: 'high' },
    { path: 'memoryModel.w[2]', label_zh: '初始稳定性 · Good', type: 'number', min: 0.1, max: 20, step: 0.01, default: 2.3065, description_zh: 'FSRS-6 w[2]：评级 Good 时的初始稳定性（天）', affects: ['retention', 'prediction'], sensitivity: 'ultra' },
    { path: 'memoryModel.w[3]', label_zh: '初始稳定性 · Easy', type: 'number', min: 0.5, max: 30, step: 0.01, default: 8.2956, description_zh: 'FSRS-6 w[3]：评级 Easy 时的初始稳定性（天）', affects: ['retention', 'prediction'], sensitivity: 'high' },
    { path: 'memoryModel.w[4]', label_zh: '难度初值偏移', type: 'number', min: 0, max: 10, step: 0.01, default: 6.4133, description_zh: 'FSRS-6 w[4]：难度初始化偏移（标准值，建议不动）' },
    { path: 'memoryModel.w[5]', label_zh: '难度初始化系数', type: 'number', min: 0, max: 5, step: 0.01, default: 0.8334, description_zh: 'FSRS-6 w[5]：难度初始化乘子（标准值，建议不动）' },
    { path: 'memoryModel.w[6]', label_zh: '难度均值回归', type: 'number', min: 0, max: 5, step: 0.01, default: 3.0194, description_zh: 'FSRS-6 w[6]：难度均值回归系数（标准值，建议不动）' },
    { path: 'memoryModel.w[7]', label_zh: '难度调节阻尼', type: 'number', min: 0, max: 1, step: 0.0001, default: 0.001, description_zh: 'FSRS-6 w[7]：难度调节阻尼（标准值，建议不动）' },
    { path: 'memoryModel.w[8]', label_zh: '稳定性增长乘子', type: 'number', min: 0.1, max: 5, step: 0.001, default: 1.8722, description_zh: 'FSRS-6 w[8]：稳定性增长基础乘子', affects: ['retention'], sensitivity: 'high' },
    { path: 'memoryModel.w[9]', label_zh: '稳定性增长幂指数', type: 'number', min: 0, max: 1, step: 0.001, default: 0.1666, description_zh: 'FSRS-6 w[9]：稳定性增长幂', affects: ['retention'], sensitivity: 'high' },
    { path: 'memoryModel.w[10]', label_zh: '间隔效应系数', type: 'number', min: 0, max: 2, step: 0.01, default: 0.796, description_zh: 'FSRS-6 w[10]：间隔效应（spacing effect）', affects: ['retention'], sensitivity: 'high' },
    { path: 'memoryModel.w[11]', label_zh: '失败稳定性下降', type: 'number', min: 0, max: 5, step: 0.01, default: 1.4835, description_zh: 'FSRS-6 w[11]：失败后稳定性下降（标准值）' },
    { path: 'memoryModel.w[12]', label_zh: '失败难度增加', type: 'number', min: 0, max: 1, step: 0.01, default: 0.0614, description_zh: 'FSRS-6 w[12]：失败后难度增加（标准值）' },
    { path: 'memoryModel.w[13]', label_zh: '失败重置系数', type: 'number', min: 0, max: 1, step: 0.01, default: 0.2629, description_zh: 'FSRS-6 w[13]：失败重置（标准值）' },
    { path: 'memoryModel.w[14]', label_zh: '失败回弹系数', type: 'number', min: 0, max: 5, step: 0.01, default: 1.6483, description_zh: 'FSRS-6 w[14]：失败回弹（标准值）' },
    { path: 'memoryModel.w[15]', label_zh: 'Hard 评级加成', type: 'number', min: 0, max: 1, step: 0.001, default: 0.6014, description_zh: 'FSRS-6 w[15]：Hard 评级稳定性加成', affects: ['retention'], sensitivity: 'med' },
    { path: 'memoryModel.w[16]', label_zh: 'Easy 评级加成', type: 'number', min: 0.5, max: 8, step: 0.01, default: 1.8729, description_zh: 'FSRS-6 w[16]：Easy 评级稳定性加成', affects: ['retention'], sensitivity: 'ultra' },
    { path: 'memoryModel.w[17]', label_zh: '同日复习缩放', type: 'number', min: 0, max: 2, step: 0.01, default: 0.5425, description_zh: 'FSRS-6 w[17]：同日复习缩放（标准值）' },
    { path: 'memoryModel.w[18]', label_zh: '同日复习偏移', type: 'number', min: 0, max: 2, step: 0.01, default: 0.0912, description_zh: 'FSRS-6 w[18]：同日复习偏移（标准值）' },
    { path: 'memoryModel.w[19]', label_zh: '同日饱和指数', type: 'number', min: 0, max: 1, step: 0.001, default: 0.0658, description_zh: 'FSRS-6 w[19]：同日复习 S^(-w19) 饱和项——小稳定性增长快、大稳定性增长慢（FSRS-6 新增）' },
    { path: 'memoryModel.w[20]', label_zh: '遗忘曲线 decay', type: 'number', min: 0.05, max: 2, step: 0.001, default: 0.1542, description_zh: 'FSRS-6 w[20]：遗忘曲线衰减指数（可训练）；factor 由 0.9^(-1/decay)-1 派生保证 R(S,S)=0.9', affects: ['retention', 'prediction'], sensitivity: 'ultra' },
    { path: 'memoryModel.baseDesiredRetention', label_zh: '目标长期留存率', type: 'number', min: 0.7, max: 0.97, step: 0.005, default: 0.9, description_zh: '调度目标留存率；越低间隔越长、复习越少、风险越大', affects: ['retention'], sensitivity: 'ultra' },
    { path: 'memoryModel.maxIntervalDays', label_zh: '最大调度间隔（天）', type: 'number', min: 30, max: 365, step: 1, default: 90, description_zh: '单词最大调度间隔上限；超过将被钳制', affects: ['retention'], sensitivity: 'ultra' },
    { path: 'memoryModel.alphaScale', label_zh: 'α 学习率缩放', type: 'number', min: 0, max: 1, step: 0.05, default: 0.3, description_zh: '难度/稳定性学习率的缩放因子' },
    { path: 'memoryModel.alphaMin', label_zh: 'α 学习率下限', type: 'number', min: 0, max: 1, step: 0.05, default: 0.1, description_zh: '每事件 α 下限；与 alphaMax 共同限定 α 区间' },
    { path: 'memoryModel.alphaMax', label_zh: 'α 学习率上限', type: 'number', min: 0, max: 1, step: 0.05, default: 0.5, description_zh: '每事件 α 上限；需 > alphaMin' },
    { path: 'memoryModel.alphaRampTau', label_zh: 'α 阻尼衰减时间常数', type: 'number', min: 0, max: 100, step: 0.5, default: 0, description_zh: '证据递减阻尼：成功复习时残余阻尼 (1-α) 随累计复习数指数衰减，复习越多越信任新证据；0 = 关闭（冻结语义）' },
    { path: 'memoryModel.retentionMin', label_zh: '自适应留存下限', type: 'number', min: 0.5, max: 0.95, step: 0.01, default: 0.75, description_zh: '根据用户状态调整后留存率下限' },
    { path: 'memoryModel.retentionMax', label_zh: '自适应留存上限', type: 'number', min: 0.7, max: 0.99, step: 0.01, default: 0.95, description_zh: '根据用户状态调整后留存率上限' },
  ],
  monitoring: [
    { path: 'monitoring.sampleRate', label_zh: '监控采样率', type: 'number', min: 0.001, max: 1, step: 0.005, default: 0.05, description_zh: '每事件落库概率；anomaly / cold-start 始终采样' },
    { path: 'monitoring.metricsFlushIntervalSecs', label_zh: '指标落盘间隔（秒）', type: 'integer', min: 30, max: 3600, step: 30, default: 300, description_zh: '内存指标聚合后写入 daily 表的间隔' },
  ],
  coldStart: [
    { path: 'coldStart.classifyToExploreEvents', label_zh: '分类→探索 事件数', type: 'integer', min: 1, max: 200, step: 1, default: 20, description_zh: '冷启动阶段从分类切换到探索所需事件数' },
    { path: 'coldStart.classifyToExploreConfidence', label_zh: '分类→探索 信心阈值', type: 'number', min: 0, max: 1, step: 0.05, default: 0.6, description_zh: '分类信心达到此值时可切换探索' },
    { path: 'coldStart.exploreToExploitEvents', label_zh: '探索→利用 事件数', type: 'integer', min: 1, max: 500, step: 1, default: 80, description_zh: '探索阶段累计事件数达到此值后切换利用' },
  ],
};

/** 所有"已精雕"的参数路径集合（不含 JSON 高级页才能编辑的剩余 ~200 项） */
export const ALL_KNOWN_PATHS: string[] = Object.values(PARAM_DICT).flatMap((g) => g.map((p) => p.path));

/** 已知 path → meta 索引 */
export const PARAM_INDEX: Map<string, ParamMeta> = new Map(
  Object.values(PARAM_DICT).flatMap((g) => g.map((p) => [p.path, p] as const)),
);

/** Tier-A path → meta */
export const TIER_A_META: ParamMeta[] = TIER_A_PATHS.map((p) => {
  const m = PARAM_INDEX.get(p);
  if (!m) throw new Error(`Tier-A path missing in dict: ${p}`);
  return m;
});

// ──────────────────────────── 取值 / 赋值（支持数组下标） ────────────────────────────

const ARRAY_PATH_RE = /^(.+?)\[(\d+)\]$/;

/** 按 path 从 AmasConfig 对象取值；找不到返回 undefined。AmasConfig 用宽松 Record 类型. */
export function getByPath(cfg: Record<string, unknown>, path: string): unknown {
  const parts = path.split('.');
  let cur: unknown = cfg;
  for (const part of parts) {
    if (cur == null || typeof cur !== 'object') return undefined;
    const m = part.match(ARRAY_PATH_RE);
    if (m) {
      const [, key, idxStr] = m;
      const arr = (cur as Record<string, unknown>)[key];
      if (!Array.isArray(arr)) return undefined;
      cur = arr[Number(idxStr)];
    } else {
      cur = (cur as Record<string, unknown>)[part];
    }
  }
  return cur;
}

/** 按 path 写值（原地，返回 cfg）。数组下标自动创建。缺失的 section 会创建。 */
export function setByPath(cfg: Record<string, unknown>, path: string, value: unknown): Record<string, unknown> {
  const parts = path.split('.');
  let cur: Record<string, unknown> = cfg;
  for (let i = 0; i < parts.length - 1; i++) {
    const part = parts[i];
    const m = part.match(ARRAY_PATH_RE);
    if (m) {
      const [, key, idxStr] = m;
      const idx = Number(idxStr);
      if (!Array.isArray(cur[key])) cur[key] = [];
      const arr = cur[key] as unknown[];
      if (typeof arr[idx] !== 'object' || arr[idx] === null) arr[idx] = {};
      cur = arr[idx] as Record<string, unknown>;
    } else {
      if (typeof cur[part] !== 'object' || cur[part] === null || Array.isArray(cur[part])) cur[part] = {};
      cur = cur[part] as Record<string, unknown>;
    }
  }
  const last = parts[parts.length - 1];
  const lm = last.match(ARRAY_PATH_RE);
  if (lm) {
    const [, key, idxStr] = lm;
    if (!Array.isArray(cur[key])) cur[key] = [];
    (cur[key] as unknown[])[Number(idxStr)] = value;
  } else {
    cur[last] = value;
  }
  return cfg;
}

// ──────────────────────────── 校验 ────────────────────────────

export interface FieldError {
  path: string;
  message: string;
}

const SUM_EPS = 0.02;

/** 单参数范围 / 类型校验，返回错误数组（空表示通过）。 */
export function validateField(meta: ParamMeta, value: unknown): string | null {
  if (meta.type === 'bool') {
    if (typeof value !== 'boolean') return '需为布尔值';
    return null;
  }
  if (typeof value !== 'number' || !Number.isFinite(value)) return '需为有限数值';
  if (meta.type === 'integer' && !Number.isInteger(value)) return '需为整数';
  if (meta.min != null && value < meta.min) return `不得小于 ${meta.min}`;
  if (meta.max != null && value > meta.max) return `不得大于 ${meta.max}`;
  return null;
}

/** 全配置预校验（含 cross-field 规则）。返回所有错误。 */
export function validateConfig(cfg: Record<string, unknown>): FieldError[] {
  const errors: FieldError[] = [];

  // 每个已知字段的范围 / 类型
  for (const meta of PARAM_INDEX.values()) {
    const v = getByPath(cfg, meta.path);
    if (v === undefined) continue; // 缺字段交给后端 default，UI 不强制
    const err = validateField(meta, v);
    if (err) errors.push({ path: meta.path, message: err });
  }

  // Cross-field 1: ensemble 三权重之和 ≈ 1
  const eh = getByPath(cfg, 'ensemble.baseWeightHeuristic') as number | undefined;
  const ei = getByPath(cfg, 'ensemble.baseWeightIge') as number | undefined;
  const es = getByPath(cfg, 'ensemble.baseWeightSwd') as number | undefined;
  if (typeof eh === 'number' && typeof ei === 'number' && typeof es === 'number') {
    const sum = eh + ei + es;
    if (Math.abs(sum - 1) > SUM_EPS) {
      errors.push({ path: 'ensemble.baseWeightHeuristic', message: `Heuristic + IGE + SWD 之和应 ≈ 1（当前 ${sum.toFixed(3)}）` });
    }
  }

  // Cross-field 2: objectiveWeights 五项之和 ≈ 1
  const ow = ['retention', 'accuracy', 'speed', 'fatigue', 'frustration']
    .map((k) => getByPath(cfg, `objectiveWeights.${k}`) as number | undefined);
  if (ow.every((v) => typeof v === 'number')) {
    const sum = (ow as number[]).reduce((a, b) => a + b, 0);
    if (Math.abs(sum - 1) > SUM_EPS) {
      errors.push({ path: 'objectiveWeights.retention', message: `五项权重之和应 ≈ 1（当前 ${sum.toFixed(3)}）` });
    }
  }

  // Cross-field 3: memoryModel.alphaMin < alphaMax
  const aMin = getByPath(cfg, 'memoryModel.alphaMin') as number | undefined;
  const aMax = getByPath(cfg, 'memoryModel.alphaMax') as number | undefined;
  if (typeof aMin === 'number' && typeof aMax === 'number' && aMin >= aMax) {
    errors.push({ path: 'memoryModel.alphaMax', message: 'alphaMax 必须大于 alphaMin' });
  }

  // Cross-field 4: retentionMin < retentionMax
  const rMin = getByPath(cfg, 'memoryModel.retentionMin') as number | undefined;
  const rMax = getByPath(cfg, 'memoryModel.retentionMax') as number | undefined;
  if (typeof rMin === 'number' && typeof rMax === 'number' && rMin >= rMax) {
    errors.push({ path: 'memoryModel.retentionMax', message: 'retentionMax 必须大于 retentionMin' });
  }

  return errors;
}

// ──────────────────────────── Preset ────────────────────────────

export type PresetId = 'factory_default';

export interface PresetInfo {
  id: PresetId;
  label_zh: string;
  description_zh: string;
}

// 2026-06-10 FSRS-6 升级：移除"已调优 2026-05-15"preset——其值属 FSRS-5 19 维空间，
// 套用到 FSRS-6 公式有害；FSRS-6 公版默认即当前最优（12 维 TPE 重调参未越过 DHP 守门）。
export const PRESETS: PresetInfo[] = [
  { id: 'factory_default', label_zh: '出厂默认', description_zh: 'AMASConfig::default()；FSRS-6 公版参数' },
];

/** 在当前 cfg 之上应用 preset 的覆盖；返回新对象。 */
export function applyPreset(cfg: Record<string, unknown>, preset: PresetId): Record<string, unknown> {
  void preset; // 目前仅 factory_default 一个 preset
  const cloned = structuredClone(cfg);
  for (const meta of PARAM_INDEX.values()) {
    setByPath(cloned, meta.path, meta.default);
  }
  return cloned;
}

// ──────────────────────────── Diff ────────────────────────────

export interface DiffEntry {
  path: string;
  label_zh: string;
  before: unknown;
  after: unknown;
}

/** 仅对"已知"参数做 diff（PARAM_DICT 范围内）；其它字段忽略 */
export function diffKnown(before: Record<string, unknown>, after: Record<string, unknown>): DiffEntry[] {
  const out: DiffEntry[] = [];
  for (const meta of PARAM_INDEX.values()) {
    const a = getByPath(before, meta.path);
    const b = getByPath(after, meta.path);
    if (!Object.is(a, b)) out.push({ path: meta.path, label_zh: meta.label_zh, before: a, after: b });
  }
  return out;
}
