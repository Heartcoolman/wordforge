use crate::store::{Store, StoreError};

type MigrationFn = fn(&Store) -> Result<(), StoreError>;

fn migrations() -> Vec<(&'static str, MigrationFn)> {
    vec![("001_initial_sqlite", m001_initial)]
}

pub fn run(store: &Store) -> Result<(), StoreError> {
    let current = get_current_version(store)?;
    let all = migrations();

    for (index, (name, func)) in all.iter().enumerate() {
        let version = (index + 1) as u32;
        if version > current {
            tracing::info!(version, name, "Running migration");
            func(store)?;
            set_version(store, version)?;
            tracing::info!(version, name, "Migration complete");
        }
    }

    Ok(())
}

pub fn get_current_version(store: &Store) -> Result<u32, StoreError> {
    let conn = store.conn()?;
    let version: u32 = conn
        .query_row(
            "SELECT version FROM schema_version WHERE singleton_id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    Ok(version)
}

pub fn set_version(store: &Store, version: u32) -> Result<(), StoreError> {
    let current = get_current_version(store)?;
    if version < current {
        return Err(StoreError::Migration {
            version,
            message: format!("Refuse to downgrade from {} to {}", current, version),
        });
    }
    let conn = store.conn()?;
    conn.execute(
        "INSERT INTO schema_version (singleton_id, version, updated_at)
         VALUES (1, ?1, datetime('now'))
         ON CONFLICT(singleton_id) DO UPDATE SET version = ?1, updated_at = datetime('now')",
        rusqlite::params![version],
    )?;
    Ok(())
}

fn m001_initial(_store: &Store) -> Result<(), StoreError> {
    // DDL is created by init_schema(); this migration just marks the version
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_is_idempotent() {
        let store = Store::open(":memory:", 5000, 1).unwrap();

        run(&store).unwrap();
        let first = get_current_version(&store).unwrap();
        run(&store).unwrap();
        let second = get_current_version(&store).unwrap();

        assert_eq!(first, 1);
        assert_eq!(second, 1);
    }

    #[test]
    fn downgrade_is_rejected() {
        let store = Store::open(":memory:", 5000, 1).unwrap();

        set_version(&store, 3).unwrap();
        let err = set_version(&store, 2).unwrap_err();
        assert!(matches!(err, StoreError::Migration { .. }));
    }
}
