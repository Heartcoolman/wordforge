# telemetry-enhanced-v1

状态：**PROPOSED**

## 包含 Spec

| 目录 | 描述 |
|------|------|
| `specs/telemetry-payload` | 遥测 payload 增强：5 秒心跳 + 设备指纹 + 行为增量 |
| `specs/heartbeat-watchdog` | 服务端看门狗：5 次丢包触发 `data_corrupted` SSE |
| `specs/telemetry-classification` | 遥测分类入库 + 管理后台结构化展示 |
| `specs/client-lockdown` | 客户端收到 `data_corrupted` 后全屏锁定 |

## 客户端需要接入的变更摘要

见 `proposal.md` 末尾「客户端需要接入的新 API」章节。
