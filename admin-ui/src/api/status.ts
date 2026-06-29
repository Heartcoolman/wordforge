import { api } from './http';

/**
 * GET /api/status 响应（匿名端点，不需 admin token）。
 * 与后端 src/routes/status.rs 对齐。
 */
export interface AppStatus {
  /** 维护模式 */
  maintenanceMode: boolean;
  /** 后端二进制版本（git version） */
  version: string | null;
  /** 当前线上 web 客户端版本 = 激活的 web-app@stable 包版本；null 表示尚未发布任何 web 构建 */
  webTargetVersion: string | null;
  /** web PWA 是否静默更新（true=静默换新；false=提示用户刷新） */
  webPwaSilentUpdate: boolean;
  /** 是否强制升级（web 不受此门控，恒 false） */
  forceUpgrade: boolean;
  /** 最新可用后端版本 */
  latestVersion: string | null;
}

export const statusApi = {
  /** 读取全局状态。匿名调用，不附带 admin token。 */
  get: () => api.get<AppStatus>('/api/status'),
};
