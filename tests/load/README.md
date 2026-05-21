# 压测脚本 (k6)

## 概述

5 条关键路径压测，验证 RFC §9.1 SLO：

| 路径 | 脚本 | SLO P95 |
|------|------|---------|
| POST /api/auth/login | login.js | ≤ 300ms |
| POST /api/learning/session | session_start.js | ≤ 500ms |
| POST /api/records/batch | review_submit.js | ≤ 400ms |
| GET /api/word-favorites | favorites.js | ≤ 200ms |
| GET /api/realtime/events | sse.js | 建连 ≤ 1s |

## 运行方式

```bash
# 单脚本干跑（不需要真实服务器，k6 --dry-run 检查语法）
k6 inspect tests/load/login.js

# 本地完整压测（需要后端在 :3000 运行）
BASE_URL=http://localhost:3000 k6 run tests/load/login.js

# 全套压测
for f in tests/load/*.js; do
  BASE_URL=http://localhost:3000 k6 run "$f"
done
```

## 负载形态

所有脚本统一采用 ramp-up → sustain → ramp-down 三段式：

- **ramp-up**：0 → peak_vus，持续 60s
- **sustain**：peak_vus 稳态，持续 60s
- **ramp-down**：peak_vus → 0，持续 30s

peak_vus 梯度：1k → 2k → 5k（每次 ramp 步长，见各脚本 stages）

## 阈值（对应 SLO）

- `http_req_duration{p(95)} < SLO_ms`：P95 延迟
- `http_req_duration{p(99)} < SLO_ms * 2`：P99 延迟（宽松 2×）
- `http_req_failed < 0.01`：错误率 < 1%
