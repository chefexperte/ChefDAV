//! IndexedDB persistence (via the `idb` crate): credentials, a per-calendar
//! event cache with sync tokens, an offline outbox, and settings.

use idb::{Database, DatabaseEvent, Factory, ObjectStoreParams, TransactionMode};
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;

const DB_NAME: &str = "chef-dav";
const STORE: &str = "kv";

async fn open() -> Result<Database, String> {
    let factory = Factory::new().map_err(|e| format!("idb factory: {e:?}"))?;
    let mut req = factory
        .open(DB_NAME, Some(1))
        .map_err(|e| format!("idb open: {e:?}"))?;
    req.on_upgrade_needed(|event| {
        let db = event.database().expect("database in upgrade callback");
        let mut params = ObjectStoreParams::new();
        params.key_path(None); // out-of-line keys
        db.create_object_store(STORE, params)
            .expect("create kv object store");
    });
    req.await.map_err(|e| format!("idb open await: {e:?}"))
}

pub async fn kv_set<T: Serialize>(key: &str, value: &T) -> Result<(), String> {
    let db = open().await?;
    let tx = db
        .transaction(&[STORE], TransactionMode::ReadWrite)
        .map_err(|e| format!("{e:?}"))?;
    let store = tx.object_store(STORE).map_err(|e| format!("{e:?}"))?;
    let js = value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| format!("serialize: {e:?}"))?;
    store
        .put(&js, Some(&JsValue::from_str(key)))
        .map_err(|e| format!("{e:?}"))?
        .await
        .map_err(|e| format!("{e:?}"))?;
    tx.commit()
        .map_err(|e| format!("{e:?}"))?
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(())
}

pub async fn kv_get<T: for<'de> Deserialize<'de>>(key: &str) -> Result<Option<T>, String> {
    let db = open().await?;
    let tx = db
        .transaction(&[STORE], TransactionMode::ReadOnly)
        .map_err(|e| format!("{e:?}"))?;
    let store = tx.object_store(STORE).map_err(|e| format!("{e:?}"))?;
    let v: Option<JsValue> = store
        .get(JsValue::from_str(key))
        .map_err(|e| format!("{e:?}"))?
        .await
        .map_err(|e| format!("{e:?}"))?;
    tx.await.map_err(|e| format!("{e:?}"))?;
    match v {
        Some(v) => serde_wasm_bindgen::from_value(v)
            .map(Some)
            .map_err(|e| format!("deserialize: {e:?}")),
        None => Ok(None),
    }
}

pub async fn kv_delete(key: &str) -> Result<(), String> {
    let db = open().await?;
    let tx = db
        .transaction(&[STORE], TransactionMode::ReadWrite)
        .map_err(|e| format!("{e:?}"))?;
    let store = tx.object_store(STORE).map_err(|e| format!("{e:?}"))?;
    store
        .delete(JsValue::from_str(key))
        .map_err(|e| format!("{e:?}"))?
        .await
        .map_err(|e| format!("{e:?}"))?;
    tx.commit()
        .map_err(|e| format!("{e:?}"))?
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(())
}

/// Credentials at rest. NOTE: stored in plain IndexedDB for now; the
/// encryption flow (`caldav::crypto`) is kept for later but not wired up.
///
/// `principal` and `home` are the discovery results from the last successful
/// login. They let a reload open the cached calendar when the backend is
/// unreachable, without asking the server again. Older entries lack them;
/// `#[serde(default)]` keeps those readable.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StoredCreds {
    pub server: String,
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub principal: Option<String>,
    #[serde(default)]
    pub home: Option<String>,
}

/// Non-secret part of the last login, kept for prefilling the form after the
/// user logs out (when the password itself is deleted).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LastLogin {
    pub server: String,
    pub username: String,
}

/// One cached event; `ics` is the raw VEVENT source for cheap re-parsing.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CachedEvent {
    pub calendar_href: String,
    pub href: String,
    pub etag: Option<String>,
    pub ics: String,
}

/// One cached task.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CachedTodo {
    pub calendar_href: String,
    pub href: String,
    pub etag: Option<String>,
    pub ics: String,
}

/// Everything cached for one calendar, plus the sync-token for incremental
/// refreshes.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CalendarCache {
    pub calendar_href: String,
    pub sync_token: Option<String>,
    pub events: Vec<CachedEvent>,
}

/// A write that could not reach the server (offline) and is retried later.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum OutboxOp {
    PutEvent {
        calendar_href: String,
        uid: String,
        ics: String,
        etag: Option<String>,
        /// Resource URL. Missing in outbox entries written before this field
        /// existed; the retry then derives it from calendar + UID as before.
        #[serde(default)]
        href: String,
    },
    DeleteEvent {
        href: String,
        etag: Option<String>,
    },
    PutTodo {
        calendar_href: String,
        uid: String,
        ics: String,
        etag: Option<String>,
        /// Resource URL; see `PutEvent::href`.
        #[serde(default)]
        href: String,
    },
    DeleteTodo {
        href: String,
        etag: Option<String>,
    },
}

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Settings {
    pub theme: String,
    pub lang: String,
    /// Keep the password in IndexedDB and sign in automatically on reload.
    /// Defaults to true so existing installs keep their saved credentials.
    #[serde(default = "default_true")]
    pub stay_logged_in: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "light".into(),
            lang: "en".into(),
            stay_logged_in: true,
        }
    }
}

pub async fn cache_load() -> Vec<CalendarCache> {
    kv_get("cache_calendars_v2").await.ok().flatten().unwrap_or_default()
}

pub async fn cache_save(caches: &[CalendarCache]) {
    let _ = kv_set("cache_calendars_v2", &caches.to_vec()).await;
}

pub async fn todos_cache_load() -> Vec<CachedTodo> {
    kv_get("cache_todos").await.ok().flatten().unwrap_or_default()
}

pub async fn todos_cache_save(todos: &[CachedTodo]) {
    let _ = kv_set("cache_todos", &todos.to_vec()).await;
}

pub async fn outbox_load() -> Vec<OutboxOp> {
    kv_get("outbox").await.ok().flatten().unwrap_or_default()
}

pub async fn outbox_save(ops: &[OutboxOp]) {
    let _ = kv_set("outbox", &ops.to_vec()).await;
}

pub async fn settings_load() -> Settings {
    kv_get("settings").await.ok().flatten().unwrap_or_default()
}

pub async fn settings_save(settings: &Settings) {
    let _ = kv_set("settings", settings).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_put_ops_without_href_still_deserialize() {
        // Entries written before the href field existed have exactly this
        // shape. They must not make the whole outbox unreadable.
        let old = r#"{"PutEvent":{"calendar_href":"https://dav/cal/","uid":"u1","ics":"BEGIN:VCALENDAR","etag":null}}"#;
        let op: OutboxOp = serde_json::from_str(old).expect("legacy outbox entry");
        match op {
            OutboxOp::PutEvent { href, uid, .. } => {
                assert_eq!(uid, "u1");
                assert!(href.is_empty());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn put_ops_keep_their_resource_href() {
        let op = OutboxOp::PutEvent {
            calendar_href: "https://dav/cal/".into(),
            uid: "u1".into(),
            ics: "BEGIN:VCALENDAR".into(),
            etag: None,
            href: "https://dav/cal/not-uid-based.ics".into(),
        };
        let json = serde_json::to_string(&op).expect("serialize");
        let back: OutboxOp = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, op);
    }
}
