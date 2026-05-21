/**
 * M2-Q1 压测：POST /api/learning/session（创建/恢复学习会话）
 * SLO（RFC §9.1）：P95 ≤ 500ms，错误率 < 1%
 *
 * 负载：ramp 1k→5k VU × 60s + sustain 60s
 * setup() 阶段先 login 拿到 token，再压测会话创建。
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
  thresholds: sloThresholds(500),
};

const TEST_EMAIL = __ENV.TEST_EMAIL || "loadtest@example.com";
const TEST_PASS  = __ENV.TEST_PASS  || "Loadtest@123";

/** 预先登录，获取 accessToken 供后续请求使用 */
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

  const res = http.post(
    `${BASE_URL}/api/learning/session`,
    JSON.stringify({}),
    { headers },
  );

  check(res, {
    "status 200 or 201": (r) => r.status === 200 || r.status === 201,
    "has sessionId": (r) => {
      try {
        const b = JSON.parse(r.body);
        return !!b.data?.id || !!b.data?.sessionId;
      } catch {
        return false;
      }
    },
  });

  sleep(1);
}
