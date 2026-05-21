/**
 * M2-Q1 压测：POST /api/records/batch（批量提交复习记录）
 * SLO（RFC §9.1）：P95 ≤ 400ms，错误率 < 1%
 *
 * 负载：ramp 1k→5k VU × 60s + sustain 60s
 * 每次 VU 提交 3 条复习记录，模拟真实学习场景。
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
  thresholds: sloThresholds(400),
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

/** 构造一批模拟复习记录 */
function makeBatch(vuId) {
  const now = new Date().toISOString();
  return [1, 2, 3].map((i) => ({
    wordId: `word_load_${vuId}_${i}`,
    result: i % 2 === 0 ? "correct" : "incorrect",
    timeSpentMs: Math.floor(Math.random() * 3000) + 500,
    reviewedAt: now,
  }));
}

export default function (data) {
  const headers = {
    ...JSON_HEADERS,
    Authorization: `Bearer ${data.token}`,
  };

  const payload = JSON.stringify({ records: makeBatch(__VU) });
  const res = http.post(`${BASE_URL}/api/records/batch`, payload, { headers });

  check(res, {
    "status 200 or 201": (r) => r.status === 200 || r.status === 201,
    "no server error": (r) => r.status < 500,
  });

  sleep(1);
}
