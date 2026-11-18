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
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StoredCreds {
    pub server: String,
    pub username: String,
    pub password: String,
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
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum OutboxOp {
    PutEvent {
        calendar_href: String,
        uid: String,
        ics: String,
        etag: Option<String>,
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
