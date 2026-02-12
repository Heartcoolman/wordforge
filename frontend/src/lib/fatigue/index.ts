/**
 * WASM 疲劳检测模块初始化
 *
 * 加载 wasm-pack 编译产物，创建各算法计算器实例。
 */

import init, {
  EARCalculator,
  PERCLOSCalculator,
  BlinkDetector,
  YawnDetector,
  HeadPoseEstimator,
  FatigueScorer,
} from '@fatigue-wasm';

import {
  FATIGUE_EAR_THRESHOLD,
  FATIGUE_EAR_SMOOTH_WINDOW,
  FATIGUE_PERCLOS_THRESHOLD,
  FATIGUE_PERCLOS_WINDOW_SECS,
  FATIGUE_BLINK_CLOSE_THRESHOLD,
  FATIGUE_BLINK_OPEN_THRESHOLD,
  FATIGUE_YAWN_MAR_THRESHOLD,
  FATIGUE_HEAD_PITCH_THRESHOLD,
  FATIGUE_HEAD_ROLL_THRESHOLD,
} from '@/lib/constants';

// 重新导出 WASM 类型，供 Worker 使用
export {
  EARCalculator,
  PERCLOSCalculator,
  BlinkDetector,
  YawnDetector,
  HeadPoseEstimator,
  FatigueScorer,
};

// 所有算法实例的集合
export interface FatigueEngine {
  earCalculator: EARCalculator;
  perclosCalculator: PERCLOSCalculator;
  blinkDetector: BlinkDetector;
  yawnDetector: YawnDetector;
  headPoseEstimator: HeadPoseEstimator;
  fatigueScorer: FatigueScorer;
}

let initialized = false;

/**
 * 初始化 WASM 模块并创建所有算法实例
 *
 * 使用推荐参数创建各计算器：
 * - EAR 阈值 0.2，平滑窗口 3
 * - PERCLOS 阈值 0.2，窗口 60 秒
 * - 眨眼阈值 0.2/0.25（迟滞防抖）
 * - 哈欠 MAR 阈值 0.6
 * - 头部下垂 pitch 15°，倾斜 roll 20°
 */
export async function initFatigueEngine(): Promise<FatigueEngine> {
  if (!initialized) {
    await init();
    initialized = true;
  }

  return {
    earCalculator: new EARCalculator(FATIGUE_EAR_THRESHOLD, FATIGUE_EAR_SMOOTH_WINDOW),
    perclosCalculator: new PERCLOSCalculator(FATIGUE_PERCLOS_THRESHOLD, FATIGUE_PERCLOS_WINDOW_SECS),
    blinkDetector: new BlinkDetector(FATIGUE_BLINK_CLOSE_THRESHOLD, FATIGUE_BLINK_OPEN_THRESHOLD),
    yawnDetector: new YawnDetector(FATIGUE_YAWN_MAR_THRESHOLD),
    headPoseEstimator: new HeadPoseEstimator(FATIGUE_HEAD_PITCH_THRESHOLD, FATIGUE_HEAD_ROLL_THRESHOLD),
    fatigueScorer: new FatigueScorer(),
  };
}

/**
 * 释放 FatigueEngine 中所有计算器的 WASM 内存
 */
export function destroyFatigueEngine(engine: FatigueEngine): void {
  engine.earCalculator.free();
  engine.perclosCalculator.free();
  engine.blinkDetector.free();
  engine.yawnDetector.free();
  engine.headPoseEstimator.free();
  engine.fatigueScorer.free();
}
