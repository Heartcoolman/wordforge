//! 全系统数据全量导出（管理员超管专用）。
//!
//! 在单连接、单读事务内遍历所有用户表，逐行经 `emit` 回调流出，保证一致性快照且不膨胀内存。

use base64::Engine;
use rusqlite::Connection;

use crate::store::{Store, StoreError};

/// 用户表清单：排除 `sqlite_%` 内部表，保留 `schema_version`（还原有用）。
fn list_dump_tables(conn: &Connection) -> Result<Vec<String>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' \
         AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// 把 SQLite `ValueRef` 转为 `serde_json::Value`。BLOB 编码为 `{"$blob_base64": "..."}`。
fn value_ref_to_json(v: rusqlite::types::ValueRef) -> serde_json::Value {
    use rusqlite::types::ValueRef::*;
    match v {
        Null => serde_json::Value::Null,
        Integer(i) => serde_json::Value::from(i),
        Real(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Text(s) => serde_json::Value::from(String::from_utf8_lossy(s).into_owned()),
        Blob(b) => {
            let encoded = base64::engine::general_purpose::STANDARD.encode(b);
            serde_json::json!({ "$blob_base64": encoded })
        }
    }
}

impl Store {
    /// 全量导出会覆盖的表清单（含空表），用于让导出产物自描述。
    pub fn dump_table_names(&self) -> Result<Vec<String>, StoreError> {
        let conn = self.conn()?;
        list_dump_tables(&conn)
    }

    /// 遍历所有用户表，逐行调用 `emit`。`emit` 返回 `false` 表示下游已断连 → 立即停止。
    /// 整库在单读事务内完成，是一致性快照。空表不产生数据行。
    pub fn stream_full_dump(
        &self,
        mut emit: impl FnMut(&str, serde_json::Value) -> bool,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute_batch("BEGIN")?; // 一致性读快照（WAL 下不阻塞写）

        let tables = list_dump_tables(&conn)?;
        for table in &tables {
            // 表名源自 sqlite_master（可信），双引号转义后内插。
            let sql = format!("SELECT * FROM \"{}\"", table.replace('"', "\"\""));
            let mut stmt = conn.prepare(&sql)?;
            let n = stmt.column_count();
            let names: Vec<String> = (0..n)
                .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
                .collect();
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let mut obj = serde_json::Map::with_capacity(n);
                for i in 0..n {
                    obj.insert(names[i].clone(), value_ref_to_json(row.get_ref(i)?));
                }
                if !emit(table, serde_json::Value::Object(obj)) {
                    return Ok(()); // 客户端断连，提前退出
                }
            }
        }

        let _ = conn.execute_batch("COMMIT");
        Ok(())
    }
}
