/**
 * 疲劳检测页面级 Hook
 *
 * 在 LearningPage / FlashcardPage 中调用，管理摄像头和 Worker 生命周期。
 */

import { onMount, onCleanup } from 'solid-js';
import { fatigueStore } from '@/stores/fatigue';
import { amasApi } from '@/api/amas';
import { CameraManager } from '@/lib/fatigue/CameraManager';
import type { WorkerCommand, WorkerResult } from '@/workers/fatigue.worker';

// 帧捕获间隔（毫秒）
const CAPTURE_INTERVAL = 100;
// 疲劳分数上报间隔（毫秒）
const REPORT_INTERVAL = 5000;

export function useFatigueDetection() {
  const camera = new CameraManager();
  let worker: Worker | null = null;
  let captureRunning = false;
  let captureAborted = false;
  let lastReportTime = 0;

  /**
   * 启动帧捕获循环：每 100ms 从视频中抓取一帧发送给 Worker
   */
  async function startCapture() {
    const video = camera.getVideo();
    if (!video) return;

    captureRunning = true;
    captureAborted = false;

    while (!captureAborted) {
      if (!worker || !camera.isActive()) break;

      try {
        const bitmap = await createImageBitmap(video);
        worker.postMessage(
          { type: 'process', bitmap, capturedAt: Date.now() } satisfies WorkerCommand,
          [bitmap],
        );
      } catch {
        // createImageBitmap 可能在视频未就绪时失败，静默忽略
      }

      await new Promise((r) => setTimeout(r, CAPTURE_INTERVAL));
    }

    captureRunning = false;
  }

  /**
   * 停止帧捕获循环
   */
  function stopCapture() {
    captureAborted = true;
  }

  /**
   * 处理 Worker 返回的消息
   */
  function handleWorkerMessage(e: MessageEvent<WorkerResult>) {
    const msg = e.data;

    switch (msg.type) {
      case 'ready':
        fatigueStore.setWasmReady(true);
        // WASM 就绪后开始帧捕获
        startCapture();
        fatigueStore.startSession();
        break;

      case 'result':
        fatigueStore.updateResult(msg.data);
        // 每 5 秒向 AMAS 后端上报视觉疲劳分数
        {
          const now = Date.now();
          if (now - lastReportTime >= REPORT_INTERVAL) {
            lastReportTime = now;
            amasApi.reportVisualFatigue(msg.data.score).catch(() => {});
          }
        }
        break;

      case 'error':
        console.error('[疲劳检测 Worker]', msg.message);
        break;
    }
  }

  /**
   * 请求摄像头权限，创建 Worker，启动检测流程
   */
  async function start() {
    if (fatigueStore.detecting()) return;
    fatigueStore.setDetecting(true);

    try {
      // 启动摄像头
      await camera.start();
      fatigueStore.setCameraReady(true);

      // 创建 Worker
      worker = new Worker(
        new URL('@/workers/fatigue.worker.ts', import.meta.url),
        { type: 'module' },
      );
      worker.onmessage = handleWorkerMessage;
      worker.onerror = (err) => {
        console.error('[疲劳检测 Worker 错误]', err.message);
      };

      // 初始化 Worker（加载 MediaPipe + WASM）
      worker.postMessage({ type: 'init' } satisfies WorkerCommand);
    } catch (err) {
      console.error('[疲劳检测启动失败]', err);
      camera.stop();
      fatigueStore.setDetecting(false);
      fatigueStore.setCameraReady(false);
    }
  }

  /**
   * 停止检测，释放摄像头和 Worker 资源
   */
  function stop() {
    stopCapture();

    if (worker) {
      worker.postMessage({ type: 'destroy' } satisfies WorkerCommand);
      worker.terminate();
      worker = null;
    }

    camera.stop();
    lastReportTime = 0;
    fatigueStore.stopSession();
    fatigueStore.setDetecting(false);
    fatigueStore.setCameraReady(false);
    fatigueStore.setWasmReady(false);
  }

  // 组件挂载时，如果用户已启用疲劳检测则自动开始
  onMount(() => {
    if (fatigueStore.enabled()) {
      start();
    }
  });

  // 组件卸载时释放所有资源
  onCleanup(() => {
    stop();
  });

  return { start, stop };
}
