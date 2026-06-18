//! 版本→schema 映射：真·任意版本回滚的核心。
//!
//! 回滚到某 release tag 前必须知道它期望的 schema 版本，才能用 down 链把"当前库副本"降级到
//! 该版本（见 `store::migrate::revert_db_copy`）。双轨解析，优先级：
//!   1. **目标二进制自声明（主路径）**：v1.2.0-beta.18+ 支持 `--print-schema-version`，
//!      回滚流程下载/解出目标二进制后试跑自报，永不漂移。
//!   2. **历史硬编码表（兜底）**：加该 flag 之前的 release。
//!
//! 关键不变式：[`HISTORICAL_SCHEMA`] **必须按版本升序排列**（[`rollback_targets`] 依赖表内
//! 位置判定"早于当前"）。表的最低项 = 支持回滚到的**最旧目标**（当前为 v1.1.0）。
//!
//! 回滚目标无需自带"启动落地能力"：watcher 会在父进程 exit 后、目标二进制启动前主动把降级好的
//! 库落地为现役库（见 services::updater::run_watcher），故早于 v1.2.0-beta.8（首个有
//! `apply_pending_rollback_db` 的版本）的目标也能承接。下界 v1.1.0 是**审计后选定**的：
//! [v1.1.0=schema21, 当前] 区间的迁移全为加表/加列型，无"原地改写既有列值"，down 降级后既有
//! 数据保持原语义。更早（pre-v1.1.0）的版本不收录、也不支持，arbitrary 输入会在下载探测后被拒。
//!
//! 数据来源：`git show <tag>:src/store/migrate.rs` 数 `migrations()` 注册条目数（= schema 版本）。

use std::path::Path;
use std::time::Duration;

/// 历史 release tag → 期望 schema 版本。覆盖 v1.1.0 至加 `--print-schema-version`(beta.18) 之前的
/// 版本；之后版本一律走二进制自声明。**必须按版本升序**（rollback_targets 依赖表序判定新旧）。
const HISTORICAL_SCHEMA: &[(&str, u32)] = &[
    ("v1.1.0-beta.1", 21),
    ("v1.1.0-beta.2", 21),
    ("v1.1.0-beta.3", 21),
    ("v1.1.0-beta.4", 21),
    ("v1.1.1", 21),
    ("v1.1.2-beta.1", 36),
    ("v1.1.2-beta.2", 36),
    ("v1.1.2-beta.3", 36),
    ("v1.1.2-beta.4", 38),
    ("v1.1.3-beta.1", 44),
    ("v1.1.3-beta.2", 44),
    ("v1.1.4-beta.1", 47),
    ("v1.1.4-beta.2", 47),
    ("v1.1.4-beta.3", 47),
    ("v1.1.4", 49),
    ("v1.1.5-beta.1", 52),
    ("v1.2.0-beta.1", 52),
    ("v1.2.0-beta.2", 52),
    ("v1.2.0-beta.3", 52),
    ("v1.2.0-beta.4", 52),
    ("v1.2.0-beta.5", 52),
    ("v1.2.0-beta.6", 52),
    ("v1.2.0-beta.7", 52),
    ("v1.2.0-beta.8", 52),
    ("v1.2.0-beta.9", 52),
    ("v1.2.0-beta.10", 52),
    ("v1.2.0-beta.11", 52),
    ("v1.2.0-beta.12", 55),
    ("v1.2.0-beta.13", 55),
    ("v1.2.0-beta.14", 58),
    ("v1.2.0-beta.15", 58),
    ("v1.2.0-beta.16", 58),
    ("v1.2.0-beta.17", 59),
];

/// 查历史硬编码表（不下载 / 不连网）。命中返回该 tag 的 schema 版本。
/// 供回滚预检在**不下载二进制**的前提下判定目标是否可解析。
pub fn schema_from_table(tag: &str) -> Option<u32> {
    HISTORICAL_SCHEMA
        .iter()
        .find(|(t, _)| *t == tag)
        .map(|(_, v)| *v)
}

/// 该 tag 是否在历史表中（= 不下载即可确定 schema、且在已审计支持的回滚下界 v1.1.0 之内）。
pub fn is_known_historical(tag: &str) -> bool {
    schema_from_table(tag).is_some()
}

/// 列出"可作为回滚目标"的已知历史版本（供 status 下发给 UI 的回滚按钮 / 任意版本下拉）：
/// 表内、严格早于 `current_tag`、且 schema `<=` 当前 schema 的版本。表按版本升序，故
/// `current_tag` 在表中时取其之前的条目即更旧版本；`current_tag` 不在表中（如 beta.18+ 新版）
/// 时表内全部条目都更旧，全部可作目标。这些目标无需下载即可解析 schema 且确定具备落地能力，
/// 故展示为按钮永不会"点了才失败"。更新的(beta.18+)目标走"任意版本"输入 + preflight 自声明。
pub fn rollback_targets(current_tag: &str, current_schema: u32) -> Vec<String> {
    let cur_idx = HISTORICAL_SCHEMA.iter().position(|(t, _)| *t == current_tag);
    HISTORICAL_SCHEMA
        .iter()
        .enumerate()
        .filter(|(i, (_, schema))| {
            *schema <= current_schema
                && match cur_idx {
                    Some(ci) => *i < ci,
                    None => true,
                }
        })
        .map(|(_, (tag, _))| tag.to_string())
        .collect()
}

/// 试跑目标二进制 `--print-schema-version` 自报 schema 版本（beta.18+）。
/// 零副作用子进程（被调二进制仅打印编译期常量后秒退），带超时。
/// 旧二进制无此 flag / 输出非法 / 超时 → 返回 None（调用方据此走兜底或拒绝）。
pub async fn probe_binary_schema(bin: &Path) -> Option<u32> {
    use std::process::Stdio;
    let fut = tokio::process::Command::new(bin)
        .arg("--print-schema-version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let out = tokio::time::timeout(Duration::from_secs(15), fut)
        .await
        .ok()?
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse::<u32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn historical_table_starts_at_v110_and_is_monotonic() {
        // 不变式：表内 schema 单调不减（版本越新 schema 越高或持平），且最低一项 = v1.1.0（下界）。
        assert_eq!(HISTORICAL_SCHEMA.first().unwrap().0, "v1.1.0-beta.1");
        assert_eq!(HISTORICAL_SCHEMA.first().unwrap().1, 21);
        let mut prev = 0u32;
        for (tag, schema) in HISTORICAL_SCHEMA {
            assert!(*schema >= prev, "{tag} schema 非单调: {schema} < {prev}");
            prev = *schema;
        }
    }

    #[test]
    fn schema_from_table_lookup() {
        assert_eq!(schema_from_table("v1.1.0-beta.1"), Some(21)); // 下界 v1.1.0
        assert_eq!(schema_from_table("v1.2.0-beta.7"), Some(52)); // beta.8 前的目标现已支持
        assert_eq!(schema_from_table("v1.2.0-beta.8"), Some(52));
        assert_eq!(schema_from_table("v1.2.0-beta.17"), Some(59));
        assert_eq!(schema_from_table("v1.0.0"), None); // pre-v1.1.0 不收录
        assert_eq!(schema_from_table("v9.9.9"), None);
    }

    #[test]
    fn rollback_targets_excludes_current_and_newer() {
        // 当前在表中(beta.17/schema 59)：目标 = 其之前的全部（含 v1.1.x），不含自身。
        let t = rollback_targets("v1.2.0-beta.17", 59);
        assert!(t.contains(&"v1.1.0-beta.1".to_string()), "应可回滚到 v1.1.0");
        assert!(t.contains(&"v1.2.0-beta.7".to_string()));
        assert!(t.contains(&"v1.2.0-beta.16".to_string()));
        assert!(!t.contains(&"v1.2.0-beta.17".to_string()));
        // 当前在表中间(beta.14)：不含更新的 beta.15/16/17。
        let t = rollback_targets("v1.2.0-beta.14", 58);
        assert!(t.contains(&"v1.1.1".to_string()));
        assert!(t.contains(&"v1.2.0-beta.13".to_string()));
        assert!(!t.contains(&"v1.2.0-beta.15".to_string()));
        assert!(!t.contains(&"v1.2.0-beta.14".to_string()));
        // 当前不在表中(beta.18+ 新版)：表内全部更旧，全部可作目标。
        let t = rollback_targets("v1.2.0-beta.18", 60);
        assert_eq!(t.len(), HISTORICAL_SCHEMA.len());
        assert!(t.contains(&"v1.2.0-beta.17".to_string()));
    }
}
