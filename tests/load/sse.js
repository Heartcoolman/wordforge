/**
 * M2-Q1 压测：GET /api/realtime/events（SSE 长连接建立）
 * SLO（RFC §9.1）：建连 P95 ≤ 1000ms，错误率 < 1%
 *
 * 负载：ramp 1k→5k VU × 60s + sustain 60s
 *
 * 注意：k6 原生 http.get 对 SSE 只验证建连握手，连接建立后立即断开。
 * 这样可以在不保持长连接的前提下压测服务器的握手吞吐量。
 * 完整 SSE 流量测试请使用 `k6/experimental/streams`（k6 v0.48+）。
 */
import http from "k6/http";
import { check, sleep } from "k6";
import { BASE_URL, JSON_HEADERS, sloThresholds, rampStages } from "./lib/common.js";

export const options = {
  stages: [
    ...rampStages(1000),
    ...rampStages(2000),
    ...rampStages(5000),
  ],
  thresholds: {
    // SSE 建连 SLO：P95 ≤ 1000ms
    http_req_duration: ["p(95)<1000", "p(99)<2000"],
    http_req_failed: ["rate<0.01"],
    // 连接建立时间（TTFB）单独监控
    "http_req_waiting{scenario:default}": ["p(95)<800"],
  },
};

const TEST_EMAIL = __ENV.TEST_EMAIL || "loadtest@example.com";
const TEST_PASS  = __ENV.TEST_PASS  || "Loadtest@123";

export function setup() {
  const res = http.post(
    `${BASE_URL}/api/auth/login`,
    JSON.stringify({ email: TEST_EMAIL, password: TEST_PASS }),
    { headers: JSON_HEADERS },
  );
  const body = JSON.parse(res.body);
  return { token: body?.data?.accessToken || "" };
}

export default function (data) {
  const headers = {
    Authorization: `Bearer ${data.token}`,
    Accept: "text/event-stream",
    "Cache-Control": "no-cache",
  };

  // 仅验证 SSE 握手（响应头），不等待事件流
  const res = http.get(`${BASE_URL}/api/realtime/events`, {
    headers,
    timeout: "5s",
    // responseType: "none" 只接收响应头，不读取 body（避免长连接阻塞 VU）
    responseType: "none",
  });

  check(res, {
    // SSE 握手成功：200 或 503（维护模式）
    "connection accepted": (r) => r.status === 200 || r.status === 503,
    "content-type is event-stream": (r) =>
      (r.headers["Content-Type"] || "").includes("text/event-stream"),
  });

  // SSE 模拟：每个 VU 建连后短暂保持再断开，不阻塞太长
  sleep(2);
}
