/**
 * k6 压测公共库：共享 BASE_URL、认证 token 获取、标准阈值构建。
 */

export const BASE_URL = __ENV.BASE_URL || "http://localhost:3000";

/** 标准 JSON POST headers */
export const JSON_HEADERS = { "Content-Type": "application/json" };

/**
 * 构建标准阈值配置。
 * @param {number} p95Ms  P95 延迟上限（毫秒）
 * @param {number} errPct 最大错误率（0-1），默认 0.01
 */
export function sloThresholds(p95Ms, errPct = 0.01) {
  return {
    http_req_duration: [
      `p(95)<${p95Ms}`,
      `p(99)<${p95Ms * 2}`,
    ],
    http_req_failed: [`rate<${errPct}`],
  };
}

/**
 * 标准 ramp-up → sustain → ramp-down 阶段，峰值 peakVUs。
 * 总时长 ≈ 150s（60 + 60 + 30）。
 */
export function rampStages(peakVUs) {
  return [
    { duration: "60s", target: peakVUs },   // ramp-up  0 → peak
    { duration: "60s", target: peakVUs },   // sustain
    { duration: "30s", target: 0 },         // ramp-down
  ];
}

/**
 * 从登录接口获取 token；返回 null 表示失败（由调用方处理）。
 * 用于 setup() 阶段，单次执行，不计入压测指标。
 */
export function fetchToken(email, password) {
  // 在 k6 setup() 中调用，使用同步 http（import 在调用方处理）
  return { email, password };
}
