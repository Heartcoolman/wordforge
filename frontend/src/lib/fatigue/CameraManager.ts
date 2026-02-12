/**
 * 摄像头生命周期管理
 *
 * 封装 getUserMedia 的获取与释放，确保资源不会泄漏。
 */

import { CAMERA_WIDTH, CAMERA_HEIGHT, CAMERA_FRAME_RATE } from '@/lib/constants';

// 默认摄像头参数
const DEFAULT_CONSTRAINTS: MediaStreamConstraints = {
  video: {
    width: CAMERA_WIDTH,
    height: CAMERA_HEIGHT,
    frameRate: CAMERA_FRAME_RATE,
    facingMode: 'user',
  },
};

export class CameraManager {
  private stream: MediaStream | null = null;
  private video: HTMLVideoElement | null = null;
  private onRevoked: (() => void) | null = null;

  /**
   * 设置权限撤销回调。
   * 当用户通过浏览器设置撤销摄像头权限时，track 的 ended 事件会触发此回调。
   */
  setOnPermissionRevoked(callback: () => void): void {
    this.onRevoked = callback;
  }

  /**
   * 请求摄像头权限并启动视频流
   *
   * @param constraints 可选的 MediaStream 约束，默认 640x480@15fps 前置
   * @returns 已绑定视频流的 HTMLVideoElement
   */
  async start(constraints?: MediaStreamConstraints): Promise<HTMLVideoElement> {
    // 如果已经在运行，先停止
    if (this.stream) {
      this.stop();
    }

    this.stream = await navigator.mediaDevices.getUserMedia(
      constraints ?? DEFAULT_CONSTRAINTS,
    );

    // 监听 track ended 事件，处理用户通过浏览器撤销摄像头权限的情况
    for (const track of this.stream.getTracks()) {
      track.addEventListener('ended', () => {
        this.stop();
        this.onRevoked?.();
      });
    }

    this.video = document.createElement('video');
    this.video.srcObject = this.stream;
    this.video.playsInline = true;
    this.video.muted = true;

    // 等待视频元数据加载完成
    await new Promise<void>((resolve, reject) => {
      const v = this.video!;
      v.onloadedmetadata = () => {
        v.play().then(() => resolve()).catch(reject);
      };
      v.onerror = () => reject(new Error('视频元素加载失败'));
    });

    return this.video;
  }

  /**
   * 停止所有视频轨道，释放摄像头资源
   */
  stop(): void {
    if (this.stream) {
      for (const track of this.stream.getTracks()) {
        track.stop();
      }
      this.stream = null;
    }

    if (this.video) {
      this.video.srcObject = null;
      this.video = null;
    }
  }

  /**
   * 摄像头是否正在运行
   */
  isActive(): boolean {
    return this.stream !== null && this.stream.active;
  }

  /**
   * 获取当前的 video 元素（可能为 null）
   */
  getVideo(): HTMLVideoElement | null {
    return this.video;
  }
}
