//! Database migration utilities
//!
//! Handles format migrations to preserve user data across versions.

use crate::error::{MemoryError, Result};
use crate::node::MemoryNode;
use rocksdb::{IteratorMode, Options, DB};
use std::path::Path;

/// Database version stored in metadata
const DB_VERSION_KEY: &[u8] = b"_db_version";
const CURRENT_VERSION: u32 = 2;

/// Check if database needs migration and perform if needed
pub fn migrate_if_needed(db_path: impl AsRef<Path>) -> Result<()> {
    let path = db_path.as_ref();
    
    // Check if database has been initialized (CURRENT file exists)
    let current_file = path.join("CURRENT");
    if !current_file.exists() {
        // Database not initialized yet, no migration needed
        log::debug!("No existing database found at {}, skipping migration", path.display());
        return Ok(());
    }

    let mut opts = Options::default();
    opts.create_if_missing(false);
    let db = DB::open(&opts, path).map_err(|e| {
        MemoryError::InvalidPath(format!("Failed to open database for migration: {}", e))
    })?;

    // Get current database version
    let current_version = match db.get(DB_VERSION_KEY)? {
        Some(bytes) => {
            let bytes_slice: &[u8] = bytes.as_ref();
            let version_bytes: [u8; 4] = bytes_slice
                .try_into()
                .map_err(|_| MemoryError::InvalidPath("Invalid version format".into()))?;
            u32::from_le_bytes(version_bytes)
        }
        None => 1, // Version 1 didn't have version key
    };

    log::info!(
        "Database version: {} (current: {})",
        current_version,
        CURRENT_VERSION
    );

    if current_version < CURRENT_VERSION {
        log::warn!(
            "Database needs migration from v{} to v{}",
            current_version,
            CURRENT_VERSION
        );
        perform_migration(&db, current_version)?;
        
        // Update version
        db.put(DB_VERSION_KEY, &CURRENT_VERSION.to_le_bytes())?;
        db.flush()?;
        
        log::info!("Migration completed successfully");
    }

    Ok(())
}

/// Perform migration from old version to current
fn perform_migration(db: &DB, from_version: u32) -> Result<()> {
    match from_version {
        1 => migrate_v1_to_v2(db)?,
        _ => {
            return Err(MemoryError::InvalidPath(format!(
                "Unknown database version: {}",
                from_version
            )))
        }
    }
    Ok(())
}

/// Migrate from v1 (JSON) to v2 (Bincode with updated format)
fn migrate_v1_to_v2(db: &DB) -> Result<()> {
    log::info!("Migrating database from v1 to v2...");
    
    let mut memories_to_migrate = Vec::new();
    let mut vectors_to_migrate = Vec::new();
    let iter = db.iterator(IteratorMode::Start);

    // Collect all entries
    for item in iter {
        let (key, value) = item.map_err(|e| {
            MemoryError::InvalidPath(format!("Failed to read database entry: {}", e))
        })?;
        let key_str = String::from_utf8_lossy(&key);

        if key_str.starts_with("mem:") {
            // Try deserializing as JSON first (v1 format)
            match serde_json::from_slice::<MemoryNode>(&value) {
                Ok(memory) => {
                    let id = key_str.strip_prefix("mem:").unwrap().to_string();
                    memories_to_migrate.push((id, memory));
                    log::debug!("Successfully parsed memory as JSON: {}", key_str);
                }
                Err(json_err) => {
                    // Try bincode (might be already migrated or corrupted)
                    match bincode::deserialize::<MemoryNode>(&value) {
                        Ok(memory) => {
                            log::debug!("Memory already in bincode format: {}", key_str);
                            let id = key_str.strip_prefix("mem:").unwrap().to_string();
                            memories_to_migrate.push((id, memory));
                        }
                        Err(bincode_err) => {
                            log::error!(
                                "Failed to deserialize memory {}: JSON error: {}, Bincode error: {}. Skipping.",
                                key_str,
                                json_err,
                                bincode_err
                            );
                            // Skip corrupted entries rather than fail entire migration
                            continue;
                        }
                    }
                }
            }
        } else if key_str.starts_with("vec:") {
            // Vectors should already be in bincode format
            match bincode::deserialize::<Vec<f32>>(&value) {
                Ok(vector) => {
                    let id = key_str.strip_prefix("vec:").unwrap().to_string();
                    vectors_to_migrate.push((id, vector));
                }
                Err(e) => {
                    log::warn!("Failed to deserialize vector {}: {}. Skipping.", key_str, e);
                }
            }
        }
    }

    log::info!(
        "Found {} memories and {} vectors to migrate",
        memories_to_migrate.len(),
        vectors_to_migrate.len()
    );

    // Reserialize memories with new bincode format
    for (id, memory) in memories_to_migrate {
        let key = format!("mem:{}", id);
        match bincode::serialize(&memory) {
            Ok(bytes) => {
                db.put(key.as_bytes(), bytes).map_err(|e| {
                    MemoryError::InvalidPath(format!("Failed to write migrated memory {}: {}", id, e))
                })?;
                log::debug!("Migrated memory: {}", id);
            }
            Err(e) => {
                log::error!("Failed to serialize memory {}: {}. Skipping.", id, e);
            }
        }
    }

    // Vectors should already be fine, but rewrite them to ensure consistency
    for (id, vector) in vectors_to_migrate {
        let key = format!("vec:{}", id);
        match bincode::serialize(&vector) {
            Ok(bytes) => {
                db.put(key.as_bytes(), bytes).map_err(|e| {
                    MemoryError::InvalidPath(format!("Failed to write migrated vector {}: {}", id, e))
                })?;
            }
            Err(e) => {
                log::error!("Failed to serialize vector {}: {}. Skipping.", id, e);
            }
        }
    }

    db.flush()
        .map_err(|e| MemoryError::InvalidPath(format!("Failed to flush database: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{MemoryId, MemoryKind, MemoryNode, MemorySource};
    use crate::temporal::TemporalMetadata;
    use tempfile::TempDir;

    #[test]
    fn test_migration_with_json_data() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path();

        // Create v1 database with JSON-serialized data
        {
            let mut opts = Options::default();
            opts.create_if_missing(true);
            let db = DB::open(&opts, db_path).unwrap();

            let memory = MemoryNode {
                id: MemoryId::new(),
                kind: MemoryKind::DebugContext {
                    problem_description: "Test problem".into(),
                    root_cause: Some("Test cause".into()),
                    solution: "Test solution".into(),
                    symptoms: vec![],
                    related_errors: vec![],
                },
                title: "Test Memory".into(),
                content: "Test content".into(),
                temporal: TemporalMetadata::now(),
                code_links: vec![],
                embedding: None,
                tags: vec![],
                source: MemorySource::default(),
                confidence: 1.0,
            };

            // Store as JSON (v1 format)
            let json_bytes = serde_json::to_vec(&memory).unwrap();
            db.put(b"mem:test-id", json_bytes).unwrap();
            db.flush().unwrap();
        }

        // Run migration
        migrate_if_needed(db_path).unwrap();

        // Verify migration succeeded
        {
            let db = DB::open_default(db_path).unwrap();
            
            // Check version was set
            let version_bytes = db.get(DB_VERSION_KEY).unwrap().unwrap();
            let version = u32::from_le_bytes(version_bytes.as_ref().try_into().unwrap());
            assert_eq!(version, CURRENT_VERSION);

            // Check memory can be deserialized with bincode
            let mem_bytes = db.get(b"mem:test-id").unwrap().unwrap();
            let _memory: MemoryNode = bincode::deserialize(&mem_bytes).unwrap();
        }
    }
}
