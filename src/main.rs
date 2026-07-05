mod logic;

use rusqlite::Connection;
use serde_json::{json, Value};
use std::io::Read;
use tiny_http::{Header, Method, Request, Response, Server};

type Resp = Response<std::io::Cursor<Vec<u8>>>;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS words(
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL DEFAULT 1,
  thai TEXT NOT NULL,
  meaning TEXT NOT NULL,
  phonetic TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS progress(
  word_id INTEGER NOT NULL,
  mode TEXT NOT NULL,
  box INTEGER NOT NULL DEFAULT 1,
  due_at INTEGER NOT NULL,
  PRIMARY KEY(word_id, mode)
);
CREATE TABLE IF NOT EXISTS sessions(
  token TEXT PRIMARY KEY,
  created_at INTEGER NOT NULL
);
";

fn main() {
    let password = std::env::var("PASSWORD").expect("set the PASSWORD env var");
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3000);
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "thai.db".into());
    let db = Connection::open(&db_path).expect("open database");
    db.execute_batch(SCHEMA).expect("create schema");

    let server = Server::http(("0.0.0.0", port)).expect("bind port");
    println!("listening on http://0.0.0.0:{port}");
    // ponytail: single-threaded request loop — fine for a handful of users,
    // the upgrade path is a thread pool or axum.
    for mut req in server.incoming_requests() {
        let resp = handle(&mut req, &db, &password);
        let _ = req.respond(resp);
    }
}

fn handle(req: &mut Request, db: &Connection, password: &str) -> Resp {
    let url = req.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((url.as_str(), ""));
    let method = req.method().clone();

    if path == "/api/login" && method == Method::Post {
        return login(req, db, password);
    }
    if path.starts_with("/api/") {
        if !authed(req, db) {
            return json_resp(401, json!({"error": "unauthorized"}));
        }
        return api(req, &method, path, query, db)
            .unwrap_or_else(|e| json_resp(400, json!({"error": e})));
    }
    static_file(path)
}

fn api(req: &mut Request, method: &Method, path: &str, query: &str, db: &Connection) -> Result<Resp, String> {
    let word_id = path.strip_prefix("/api/words/").and_then(|s| s.parse::<i64>().ok());
    match (method, path) {
        (Method::Get, "/api/words") => list_words(db),
        (Method::Post, "/api/words") => {
            let (thai, meaning, phonetic) = word_fields(&body_json(req)?)?;
            db.execute(
                "INSERT INTO words(thai, meaning, phonetic, created_at) VALUES (?1, ?2, ?3, ?4)",
                (&thai, &meaning, &phonetic, now()),
            ).map_err(db_err)?;
            Ok(json_resp(200, json!({"id": db.last_insert_rowid()})))
        }
        (Method::Put, _) if word_id.is_some() => {
            let (thai, meaning, phonetic) = word_fields(&body_json(req)?)?;
            let n = db.execute(
                "UPDATE words SET thai = ?1, meaning = ?2, phonetic = ?3 WHERE id = ?4",
                (&thai, &meaning, &phonetic, word_id),
            ).map_err(db_err)?;
            if n == 0 { return Err("no such word".into()); }
            Ok(json_resp(200, json!({"ok": true})))
        }
        (Method::Delete, _) if word_id.is_some() => {
            db.execute("DELETE FROM progress WHERE word_id = ?1", [word_id]).map_err(db_err)?;
            db.execute("DELETE FROM words WHERE id = ?1", [word_id]).map_err(db_err)?;
            Ok(json_resp(200, json!({"ok": true})))
        }
        (Method::Get, "/api/quiz") => {
            let mode = query.split('&').find_map(|kv| kv.strip_prefix("mode=")).unwrap_or("");
            check_mode(mode)?;
            quiz(db, mode)
        }
        (Method::Post, "/api/review") => review(db, &body_json(req)?),
        _ => Err("no such endpoint".into()),
    }
}

fn list_words(db: &Connection) -> Result<Resp, String> {
    let mut boxes: std::collections::HashMap<i64, Value> = std::collections::HashMap::new();
    let mut stmt = db.prepare("SELECT word_id, mode, box FROM progress").map_err(db_err)?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))
        .map_err(db_err)?;
    for row in rows {
        let (wid, mode, b) = row.map_err(db_err)?;
        boxes.entry(wid).or_insert_with(|| json!({}))[mode] = json!(b);
    }

    let mut stmt = db.prepare("SELECT id, thai, meaning, phonetic FROM words ORDER BY id DESC").map_err(db_err)?;
    let words = stmt.query_map([], |r| {
        Ok(json!({
            "id": r.get::<_, i64>(0)?,
            "thai": r.get::<_, String>(1)?,
            "meaning": r.get::<_, String>(2)?,
            "phonetic": r.get::<_, String>(3)?,
        }))
    }).map_err(db_err)?
      .collect::<Result<Vec<_>, _>>().map_err(db_err)?
      .into_iter()
      .map(|mut w| {
          let id = w["id"].as_i64().unwrap();
          w["boxes"] = boxes.remove(&id).unwrap_or_else(|| json!({}));
          w
      })
      .collect::<Vec<_>>();
    Ok(json_resp(200, json!(words)))
}

fn quiz(db: &Connection, mode: &str) -> Result<Resp, String> {
    let mut stmt = db.prepare(
        "SELECT w.id, w.thai, w.meaning, w.phonetic, COALESCE(p.box, 1)
         FROM words w LEFT JOIN progress p ON p.word_id = w.id AND p.mode = ?1
         WHERE (p.due_at IS NULL OR p.due_at <= ?2)
           AND NOT (?1 = 'phonetic' AND w.phonetic = '')
         ORDER BY RANDOM() LIMIT 20",
    ).map_err(db_err)?;
    let words = stmt.query_map((mode, now()), |r| {
        Ok(json!({
            "id": r.get::<_, i64>(0)?,
            "thai": r.get::<_, String>(1)?,
            "meaning": r.get::<_, String>(2)?,
            "phonetic": r.get::<_, String>(3)?,
            "box": r.get::<_, i64>(4)?,
        }))
    }).map_err(db_err)?
      .collect::<Result<Vec<_>, _>>().map_err(db_err)?;
    Ok(json_resp(200, json!(words)))
}

fn review(db: &Connection, body: &Value) -> Result<Resp, String> {
    let word_id = body["word_id"].as_i64().ok_or("word_id required")?;
    let mode = body["mode"].as_str().unwrap_or("");
    let correct = body["correct"].as_bool().ok_or("correct required")?;
    check_mode(mode)?;
    db.query_row("SELECT 1 FROM words WHERE id = ?1", [word_id], |_| Ok(()))
        .map_err(|_| "no such word".to_string())?;

    let current: i64 = db.query_row(
        "SELECT box FROM progress WHERE word_id = ?1 AND mode = ?2",
        (word_id, mode), |r| r.get(0),
    ).unwrap_or(1);
    let (new_box, secs) = logic::leitner(current, correct);
    db.execute(
        "INSERT INTO progress(word_id, mode, box, due_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(word_id, mode) DO UPDATE SET box = ?3, due_at = ?4",
        (word_id, mode, new_box, now() + secs),
    ).map_err(db_err)?;
    Ok(json_resp(200, json!({"box": new_box})))
}

fn login(req: &mut Request, db: &Connection, password: &str) -> Resp {
    let given = body_json(req).ok()
        .and_then(|b| b["password"].as_str().map(String::from))
        .unwrap_or_default();
    if !constant_time_eq(given.as_bytes(), password.as_bytes()) {
        return json_resp(401, json!({"error": "wrong password"}));
    }
    let token = random_token();
    db.execute("INSERT INTO sessions(token, created_at) VALUES (?1, ?2)", (&token, now())).unwrap();
    let cookie = format!("session={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age=31536000");
    json_resp(200, json!({"ok": true}))
        .with_header(Header::from_bytes(&b"Set-Cookie"[..], cookie.as_bytes()).unwrap())
}

fn authed(req: &Request, db: &Connection) -> bool {
    let Some(cookie) = req.headers().iter().find(|h| h.field.equiv("Cookie")) else { return false };
    let value = cookie.value.as_str().to_string();
    let Some(token) = value.split(';').find_map(|c| c.trim().strip_prefix("session=")) else { return false };
    db.query_row("SELECT 1 FROM sessions WHERE token = ?1", [token], |_| Ok(())).is_ok()
}

fn static_file(path: &str) -> Resp {
    let rel = if path == "/" { "index.html" } else { path.trim_start_matches('/') };
    if rel.contains("..") {
        return json_resp(404, json!({"error": "not found"}));
    }
    let full = std::path::Path::new("public").join(rel);
    match std::fs::read(&full) {
        Ok(data) => {
            let ct = match full.extension().and_then(|e| e.to_str()) {
                Some("html") => "text/html; charset=utf-8",
                Some("js") => "text/javascript",
                Some("css") => "text/css",
                Some("json") => "application/json",
                Some("svg") => "image/svg+xml",
                _ => "application/octet-stream",
            };
            Response::from_data(data)
                .with_header(Header::from_bytes(&b"Content-Type"[..], ct.as_bytes()).unwrap())
        }
        Err(_) => json_resp(404, json!({"error": "not found"})),
    }
}

// ---- helpers ----

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64
}

fn json_resp(status: u32, v: Value) -> Resp {
    Response::from_data(serde_json::to_vec(&v).unwrap())
        .with_status_code(status)
        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap())
}

fn body_json(req: &mut Request) -> Result<Value, String> {
    let mut s = String::new();
    req.as_reader().take(65536).read_to_string(&mut s).map_err(|_| "unreadable body")?;
    serde_json::from_str(&s).map_err(|_| "invalid json".into())
}

/// Extracts and validates word fields; thai and meaning must be non-empty.
fn word_fields(body: &Value) -> Result<(String, String, String), String> {
    let thai = body["thai"].as_str().unwrap_or("").trim().to_string();
    let meaning = body["meaning"].as_str().unwrap_or("").trim().to_string();
    let phonetic = body["phonetic"].as_str().unwrap_or("").trim().to_string();
    if thai.is_empty() || meaning.is_empty() {
        return Err("thai and meaning are required".into());
    }
    Ok((thai, meaning, phonetic))
}

fn check_mode(mode: &str) -> Result<(), String> {
    if logic::MODES.contains(&mode) { Ok(()) } else { Err(format!("unknown mode '{mode}'")) }
}

fn db_err(e: rusqlite::Error) -> String {
    e.to_string()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    // Leaks only the length, which is acceptable for a password check.
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn random_token() -> String {
    let mut buf = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut buf))
        .expect("read /dev/urandom");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}
