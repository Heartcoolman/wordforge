# 维护模式运维 SOP

> **状态**：v1.0 已实装（后端 + admin 前端 UI），v1.1-P2.9 验证确认
> **范围**：发版灰度暂停、大型迁移期间冻结写入、紧急止血

---

## 一、链路图

```
admin 前端 SettingsPage Switch
      ↓ POST /api/admin/settings/maintenance { active: true }
      ↓
src/routes/admin/settings.rs::set_maintenance
      ↓ ① 更新 system_settings.maintenance_mode 列
      ↓ ② state.set_maintenance(true)
      ↓ ③ state.maintenance_tx.send(true)
      ↓
src/routes/realtime.rs SSE 通道
      ↓ event: "maintenance" data: {"active":true}
      ↓
所有在线客户端 → AppState.handleRealtime → UI 显示维护横幅
```

同时，后续所有受 `maintenance::maintenance_middleware` 保护的路由会立即返回 `503 SERVICE_UNAVAILABLE`。豁免路由（admin / status / realtime / telemetry / v1）不受影响。

---

## 二、开启维护模式

1. admin 后台 → "系统设置" 页 → 找到"维护模式"开关
2. 拨到 ON → 弹出 ConfirmDialog 二次确认
3. 确认后约 1 秒内：
   - 所有客户端 SSE 立即收到 `maintenance` 事件
   - 业务路由开始返 503
   - admin 后台、SSE、health/metrics 不受影响

## 三、关闭维护模式

1. admin 后台 SettingsPage 把开关拨到 OFF（不需要确认）
2. 业务路由立即恢复

## 四、应急 CLI

如果 admin 后台不可用（如前端 build 故障），用 CLI 直接改 DB：

```bash
# 开启
sqlite3 /opt/wordforge/data/learning.db \
  "UPDATE system_settings SET maintenance_mode = 1;"
systemctl restart wordforge   # 重启让 AppState 重新读 maintenance_mode

# 关闭
sqlite3 /opt/wordforge/data/learning.db \
  "UPDATE system_settings SET maintenance_mode = 0;"
systemctl restart wordforge
```

注意：直接改 DB 不会推 SSE 事件（因为绕过了 state 通路）。客户端只会在下次轮询时感知。建议优先用 admin 后台开关。

## 五、与发版协调

- **资源包热更（v1.1-P0）**：admin 切 channel active 触发 `resource_pack_available` SSE，客户端自动下载。维护期间应**先关 SSE**（即先开维护模式让客户端断开），再切 channel，避免下载到一半被服务端打断
- **二进制自更新**：`admin/updates apply` 内部会自动开维护模式 → 升级 → 关维护模式（M0-R4 实装）
- **大型 schema 迁移**：手动开维护模式 → 跑迁移 → 验证 → 关维护模式

## 六、可观测

- Prometheus：`maintenance_mode_active{job="wordforge"}` gauge（0/1）
- 日志：每次切换在 `tracing::info!` 打 `action=set_maintenance` 行
- audit：v1.1-P2.10 后会写入 `update_audit_log`（target_type="system", target_id="maintenance"）
