/**
 * M2-Q1 压测：GET /api/word-favorites（收藏列表分页读取）
 * SLO（RFC §9.1）：P95 ≤ 200ms，错误率 < 1%
 *
 * 负载：ramp 1k→5k VU × 60s + sustain 60s
 * 只读路径，是高频访问场景，SLO 最严苛。
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
  thresholds: sloThresholds(200),
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
    ...JSON_HEADERS,
    Authorization: `Bearer ${data.token}`,
  };

  const res = http.get(
    `${BASE_URL}/api/word-favorites?page=1&pageSize=20`,
    { headers },
  );

  check(res, {
    "status 200": (r) => r.status === 200,
    "has data array": (r) => {
      try {
        const b = JSON.parse(r.body);
        return Array.isArray(b.data?.data) || Array.isArray(b.data);
      } catch {
        return false;
      }
    },
  });

  sleep(0.5);
}
