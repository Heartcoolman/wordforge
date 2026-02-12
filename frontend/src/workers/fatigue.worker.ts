/**
 * 疲劳检测 Web Worker
 *
 * 在独立线程中运行 MediaPipe FaceLandmarker + Rust WASM 疲劳算法，
 * 避免阻塞主线程渲染。
 */

import { FaceLandmarker, FilesetResolver } from '@mediapipe/tasks-vision';
import { initFatigueEngine, destroyFatigueEngine, type FatigueEngine } from '@/lib/fatigue';
import type { FatigueResult } from '@/stores/fatigue';
import { MEDIAPIPE_CDN_URLS, MEDIAPIPE_MODEL_ASSET_PATH } from '@/lib/constants';

// --- 消息协议 ---

// 主线程 → Worker
export type WorkerCommand =
  | { type: 'init' }
  | { type: 'process'; bitmap: ImageBitmap }
  | { type: 'reset' }
  | { type: 'destroy' };

// Worker → 主线程
export type WorkerResult =
  | { type: 'ready' }
  | { type: 'result'; data: FatigueResult }
  | { type: 'error'; message: string };

// --- MediaPipe 面部关键点索引 (478 点模型) ---

// 左眼 6 点 (用于 6 点 EAR 计算)
const LEFT_EYE_IDX = [33, 160, 158, 133, 153, 144] as const;
// 右眼 6 点
const RIGHT_EYE_IDX = [362, 385, 387, 263, 373, 380] as const;
// 嘴部 8 点 (用于 MAR 计算)
const MOUTH_IDX = [61, 291, 39, 181, 0, 17, 269, 405] as const;

// --- 状态 ---

let faceLandmarker: FaceLandmarker | null = null;
let engine: FatigueEngine | null = null;

/**
 * 从 478 个关键点中提取指定索引的 2D 坐标（仅 x, y）
 * 返回 Float64Array: [x0, y0, x1, y1, ...]
 * WASM 的 EAR/MAR 算法使用 2D 坐标
 */
function extract2DLandmarks(
  allLandmarks: Array<{ x: number; y: number; z: number }>,
  indices: readonly number[],
): Float64Array {
  const result = new Float64Array(indices.length * 2);
  for (let i = 0; i < indices.length; i++) {
    const lm = allLandmarks[indices[i]];
    result[i * 2] = lm.x;
    result[i * 2 + 1] = lm.y;
  }
  return result;
}

/**
 * 从 MediaPipe 4x4 变换矩阵提取欧拉角 (pitch/yaw/roll)
 *
 * 矩阵为行主序 (row-major)：
 * [r00, r01, r02, tx, r10, r11, r12, ty, r20, r21, r22, tz, 0, 0, 0, 1]
 *
 * ZYX 顺序欧拉角分解：
 * - pitch (绕X轴) = atan2(r21, r22)
 * - yaw   (绕Y轴) = asin(-r20)
 * - roll  (绕Z轴) = atan2(r10, r00)
 */
function matrixToEulerAngles(data: ArrayLike<number>): { pitch: number; yaw: number; roll: number } {
  const RAD2DEG = 180 / Math.PI;

  const r00 = data[0];
  const r10 = data[4];
  const r20 = data[8];
  const r21 = data[9];
  const r22 = data[10];

  const pitch = Math.atan2(r21, r22) * RAD2DEG;
  const yaw = Math.asin(-Math.max(-1, Math.min(1, r20))) * RAD2DEG;
  const roll = Math.atan2(r10, r00) * RAD2DEG;

  return { pitch, yaw, roll };
}

/**
 * 从 MediaPipe blendshapes 计算表情疲劳分数
 *
 * 关注与疲劳相关的 blendshape 类别：
 * - eyeBlinkLeft / eyeBlinkRight: 眨眼强度
 * - eyeSquintLeft / eyeSquintRight: 眯眼
 * - browDownLeft / browDownRight: 皱眉
 */
function computeExpressionFatigue(
  blendshapes: { categories: Array<{ categoryName: string; score: number }> },
): number {
  const EXPRESSION_FATIGUE_WEIGHTS: Record<string, number> = {
    eyeSquintLeft: 0.3,
    eyeSquintRight: 0.3,
    browDownLeft: 0.2,
    browDownRight: 0.2,
  };

  let score = 0;
  let totalWeight = 0;

  for (const cat of blendshapes.categories) {
    const weight = EXPRESSION_FATIGUE_WEIGHTS[cat.categoryName];
    if (weight !== undefined) {
      score += cat.score * weight;
      totalWeight += weight;
    }
  }

  return totalWeight > 0 ? Math.min(score / totalWeight, 1.0) : 0;
}

/**
 * 初始化 MediaPipe FaceLandmarker 和 WASM 疲劳引擎
 */
/**
 * 尝试多个 CDN 加载 MediaPipe Vision，返回第一个成功的结果
 */
async function resolveVisionWithFallback() {
  for (const cdnUrl of MEDIAPIPE_CDN_URLS) {
    try {
      return await FilesetResolver.forVisionTasks(cdnUrl);
    } catch {
      // 当前 CDN 失败，尝试下一个
    }
  }
  throw new Error('所有 CDN 均无法加载 MediaPipe Vision WASM');
}

async function handleInit(): Promise<void> {
  try {
    // 并行加载 MediaPipe 和 WASM
    const [vision, wasmEngine] = await Promise.all([
      resolveVisionWithFallback(),
      initFatigueEngine(),
    ]);

    // 尝试多个 CDN 加载模型资源
    let landmarker: FaceLandmarker | null = null;
    for (const cdnUrl of MEDIAPIPE_CDN_URLS) {
      try {
        const modelUrl = new URL(MEDIAPIPE_MODEL_ASSET_PATH, `${cdnUrl}/`).toString();
        landmarker = await FaceLandmarker.createFromOptions(vision, {
          baseOptions: {
            modelAssetPath: modelUrl,
            delegate: 'CPU',
          },
          runningMode: 'IMAGE',
          numFaces: 1,
          outputFaceBlendshapes: true,
          outputFacialTransformationMatrixes: true,
        });
        break;
      } catch {
        // 当前 CDN 模型加载失败，尝试下一个
      }
    }

    if (!landmarker) {
      throw new Error('所有 CDN 均无法加载 FaceLandmarker 模型');
    }

    faceLandmarker = landmarker;

    engine = wasmEngine;

    self.postMessage({ type: 'ready' } satisfies WorkerResult);
  } catch (err) {
    const message = err instanceof Error ? err.message : '初始化失败';
    self.postMessage({ type: 'error', message } satisfies WorkerResult);
  }
}

/**
 * 处理单帧图像
 */
function handleProcess(bitmap: ImageBitmap): void {
  try {
    if (!faceLandmarker || !engine) {
      bitmap.close();
      return;
    }

    // MediaPipe 面部关键点检测
    const result = faceLandmarker.detect(bitmap);
    bitmap.close();

    if (!result.faceLandmarks || result.faceLandmarks.length === 0) {
      return;
    }

    const landmarks = result.faceLandmarks[0];
    const now = Date.now();

    // === EAR 计算 ===
    // 提取 2D 坐标 (x,y)，每眼 6 点 × 2 = 12 个 float
    const leftEyeCoords = extract2DLandmarks(landmarks, LEFT_EYE_IDX);
    const rightEyeCoords = extract2DLandmarks(landmarks, RIGHT_EYE_IDX);

    const leftEAR = engine.earCalculator.calculate6Point(leftEyeCoords);
    const rightEAR = engine.earCalculator.calculate6Point(rightEyeCoords);
    const avgEAR = (leftEAR.ear + rightEAR.ear) / 2;
    leftEAR.free();
    rightEAR.free();

    // === PERCLOS ===
    const perclos = engine.perclosCalculator.update(avgEAR, now);

    // === 眨眼检测 ===
    const blinkResult = engine.blinkDetector.update(avgEAR, now);
    const blinkRate = blinkResult.blink_rate;
    const blinkAbnormal = blinkResult.is_abnormal;
    blinkResult.free();

    // === 哈欠检测 ===
    // 提取 2D 嘴部坐标，8 点 × 2 = 16 个 float
    const mouthCoords = extract2DLandmarks(landmarks, MOUTH_IDX);
    const yawnResult = engine.yawnDetector.update(mouthCoords, now);
    const yawnCount = yawnResult.yawn_count;
    const yawnRate = yawnResult.yawn_rate;
    yawnResult.free();

    // === 头部姿态 ===
    let pitch = 0;
    let yaw = 0;
    let roll = 0;
    if (result.facialTransformationMatrixes && result.facialTransformationMatrixes.length > 0) {
      const matrix = result.facialTransformationMatrixes[0];
      const angles = matrixToEulerAngles(matrix.data);
      pitch = angles.pitch;
      yaw = angles.yaw;
      roll = angles.roll;
    }
    const headResult = engine.headPoseEstimator.update(pitch, yaw, roll, now);
    const headDropRatio = headResult.drop_ratio;
    headResult.free();

    // === 表情评分 ===
    let expressionScore = 0;
    if (result.faceBlendshapes && result.faceBlendshapes.length > 0) {
      expressionScore = computeExpressionFatigue(result.faceBlendshapes[0]);
    }

    // === 综合疲劳评分 ===
    // FatigueScorer.calculate() 返回 serde 序列化的 JsValue（已含 camelCase 字段）
    const fatigueJsValue = engine.fatigueScorer.calculate(
      perclos,
      blinkRate,
      blinkAbnormal,
      yawnCount,
      yawnRate,
      headDropRatio,
      expressionScore,
      now,
    );

    // serde-wasm-bindgen 序列化结果已经是普通 JS 对象
    const fatigueResult = fatigueJsValue as FatigueResult;

    self.postMessage({ type: 'result', data: fatigueResult } satisfies WorkerResult);
  } catch (err) {
    const message = err instanceof Error ? err.message : '处理帧数据时出错';
    self.postMessage({ type: 'error', message } satisfies WorkerResult);
  }
}

/**
 * 重置所有 WASM 计算器状态
 */
function handleReset(): void {
  if (engine) {
    engine.earCalculator.reset();
    engine.perclosCalculator.reset();
    engine.blinkDetector.reset();
    engine.yawnDetector.reset();
    engine.headPoseEstimator.reset();
    engine.fatigueScorer.reset();
  }
}

/**
 * 释放所有资源
 */
function handleDestroy(): void {
  if (faceLandmarker) {
    faceLandmarker.close();
    faceLandmarker = null;
  }
  if (engine) {
    destroyFatigueEngine(engine);
    engine = null;
  }
}

// --- 消息监听 ---

self.onmessage = async (e: MessageEvent<WorkerCommand>) => {
  const cmd = e.data;

  switch (cmd.type) {
    case 'init':
      await handleInit();
      break;
    case 'process':
      handleProcess(cmd.bitmap);
      break;
    case 'reset':
      handleReset();
      break;
    case 'destroy':
      handleDestroy();
      break;
  }
};
