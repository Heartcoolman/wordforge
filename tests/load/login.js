/**
 * M2-Q1 压测：POST /api/auth/login
 * SLO（RFC §9.1）：P95 ≤ 300ms，错误率 < 1%
 *
 * 负载：ramp 1k→5k VU × 60s + sustain 60s
 */
import http from "k6/http";
import { check, sleep } from "k6";
import { BASE_URL, JSON_HEADERS, sloThresholds, rampStages } from "./lib/common.js";

// 峰值 VU 梯度：1k → 2k → 5k
export const options = {
  stages: [
    ...rampStages(1000),   // 第一梯度：峰值 1k VU
    ...rampStages(2000),   // 第二梯度：峰值 2k VU
    ...rampStages(5000),   // 第三梯度：峰值 5k VU
  ],
  thresholds: sloThresholds(300),
};

const LOGIN_URL = `${BASE_URL}/api/auth/login`;

// 用于压测的虚拟用户凭据（后端 seeded test user）
const TEST_EMAIL = __ENV.TEST_EMAIL || "loadtest@example.com";
const TEST_PASS  = __ENV.TEST_PASS  || "Loadtest@123";

export default function () {
  const payload = JSON.stringify({ email: TEST_EMAIL, password: TEST_PASS });

  const res = http.post(LOGIN_URL, payload, { headers: JSON_HEADERS });

  check(res, {
    "status 200": (r) => r.status === 200,
    "has accessToken": (r) => {
      try {
        return !!JSON.parse(r.body).data?.accessToken;
      } catch {
        return false;
      }
    },
  });

  sleep(1);
}
