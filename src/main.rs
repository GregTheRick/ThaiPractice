mod logic;

use rusqlite::Connection;
use serde_json::{json, Value};
use std::io::Read;
use tiny_http::{Header, Method, Request, Response, Server};

type Resp = Response<std::io::Cursor<Vec<u8>>>;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS users(
  id INTEGER PRIMARY KEY,
  username TEXT NOT NULL UNIQUE COLLATE NOCASE,
  password_hash TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS words(
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL DEFAULT 1,
  thai TEXT NOT NULL,
  meaning TEXT NOT NULL, -- JSON list of meanings, e.g. [\"go to\",\"go with\"]
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
  user_id INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);
";

fn main() {
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3000);
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "thai.db".into());
    let db = Connection::open(&db_path).expect("open database");
    // migrate pre-multi-user databases: their sessions table has no user_id,
    // so drop it (forces one re-login) and let SCHEMA recreate it. Words kept
    // user_id=1, which the first registered account receives.
    if db.prepare("SELECT user_id FROM sessions LIMIT 1").is_err() {
        db.execute_batch("DROP TABLE IF EXISTS sessions").expect("migrate sessions");
    }
    db.execute_batch(SCHEMA).expect("create schema");
    migrate_meanings(&db);

    let server = Server::http(("0.0.0.0", port)).expect("bind port");
    println!("listening on http://0.0.0.0:{port}");
    // ponytail: single-threaded request loop — fine for a handful of users,
    // the upgrade path is a thread pool or axum.
    for mut req in server.incoming_requests() {
        let resp = handle(&mut req, &db);
        let _ = req.respond(resp);
    }
}

/// Converts pre-list plain-text meanings ("water; liquid") to JSON lists.
fn migrate_meanings(db: &Connection) {
    let mut stmt = db.prepare("SELECT id, meaning FROM words").expect("read words");
    let rows: Vec<(i64, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("read words")
        .flatten()
        .collect();
    drop(stmt);
    for (id, raw) in rows {
        if serde_json::from_str::<Vec<String>>(&raw).is_err() {
            let list: Vec<&str> = raw.split([';', ',']).map(str::trim).filter(|s| !s.is_empty()).collect();
            db.execute(
                "UPDATE words SET meaning = ?1 WHERE id = ?2",
                (serde_json::to_string(&list).unwrap(), id),
            ).expect("migrate meanings");
        }
    }
}

fn handle(req: &mut Request, db: &Connection) -> Resp {
    let url = req.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((url.as_str(), ""));
    let method = req.method().clone();

    if method == Method::Post && (path == "/api/login" || path == "/api/register") {
        return account(req, db, path == "/api/register");
    }
    if path.starts_with("/api/") {
        let Some((user_id, token)) = session(req, db) else {
            return json_resp(401, json!({"error": "unauthorized"}));
        };
        if method == Method::Post && path == "/api/logout" {
            return logout(db, &token);
        }
        return api(req, &method, path, query, db, user_id)
            .unwrap_or_else(|e| json_resp(400, json!({"error": e})));
    }
    static_file(path)
}

fn api(req: &mut Request, method: &Method, path: &str, query: &str, db: &Connection, user_id: i64) -> Result<Resp, String> {
    let word_id = path.strip_prefix("/api/words/").and_then(|s| s.parse::<i64>().ok());
    match (method, path) {
        (Method::Get, "/api/me") => {
            let username: String = db
                .query_row("SELECT username FROM users WHERE id = ?1", [user_id], |r| r.get(0))
                .map_err(db_err)?;
            Ok(json_resp(200, json!({"username": username})))
        }
        (Method::Get, "/api/words") => list_words(db, user_id),
        (Method::Post, "/api/words") => {
            let (thai, meaning, phonetic) = word_fields(&body_json(req)?)?;
            db.execute(
                "INSERT INTO words(user_id, thai, meaning, phonetic, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                (user_id, &thai, &meaning, &phonetic, now()),
            ).map_err(db_err)?;
            Ok(json_resp(200, json!({"id": db.last_insert_rowid()})))
        }
        (Method::Put, _) if word_id.is_some() => {
            let (thai, meaning, phonetic) = word_fields(&body_json(req)?)?;
            let n = db.execute(
                "UPDATE words SET thai = ?1, meaning = ?2, phonetic = ?3 WHERE id = ?4 AND user_id = ?5",
                (&thai, &meaning, &phonetic, word_id, user_id),
            ).map_err(db_err)?;
            if n == 0 { return Err("no such word".into()); }
            Ok(json_resp(200, json!({"ok": true})))
        }
        (Method::Delete, _) if word_id.is_some() => {
            let n = db.execute("DELETE FROM words WHERE id = ?1 AND user_id = ?2", (word_id, user_id))
                .map_err(db_err)?;
            if n == 0 { return Err("no such word".into()); }
            db.execute("DELETE FROM progress WHERE word_id = ?1", [word_id]).map_err(db_err)?;
            Ok(json_resp(200, json!({"ok": true})))
        }
        (Method::Get, "/api/quiz") => {
            let mode = query.split('&').find_map(|kv| kv.strip_prefix("mode=")).unwrap_or("");
            check_mode(mode)?;
            quiz(db, mode, user_id)
        }
        (Method::Post, "/api/review") => review(db, &body_json(req)?, user_id),
        _ => Err("no such endpoint".into()),
    }
}

fn list_words(db: &Connection, user_id: i64) -> Result<Resp, String> {
    let mut boxes: std::collections::HashMap<i64, Value> = std::collections::HashMap::new();
    let mut stmt = db.prepare(
        "SELECT p.word_id, p.mode, p.box FROM progress p
         JOIN words w ON w.id = p.word_id WHERE w.user_id = ?1",
    ).map_err(db_err)?;
    let rows = stmt.query_map([user_id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))
        .map_err(db_err)?;
    for row in rows {
        let (wid, mode, b) = row.map_err(db_err)?;
        boxes.entry(wid).or_insert_with(|| json!({}))[mode] = json!(b);
    }

    let mut stmt = db.prepare("SELECT id, thai, meaning, phonetic FROM words WHERE user_id = ?1 ORDER BY id DESC")
        .map_err(db_err)?;
    let words = stmt.query_map([user_id], |r| {
        Ok(json!({
            "id": r.get::<_, i64>(0)?,
            "thai": r.get::<_, String>(1)?,
            "meanings": meanings_json(r.get(2)?),
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

fn quiz(db: &Connection, mode: &str, user_id: i64) -> Result<Resp, String> {
    let mut stmt = db.prepare(
        "SELECT w.id, w.thai, w.meaning, w.phonetic, COALESCE(p.box, 1)
         FROM words w LEFT JOIN progress p ON p.word_id = w.id AND p.mode = ?1
         WHERE w.user_id = ?3
           AND (p.due_at IS NULL OR p.due_at <= ?2)
           AND NOT (?1 = 'phonetic' AND w.phonetic = '')
         ORDER BY RANDOM() LIMIT 20",
    ).map_err(db_err)?;
    let words = stmt.query_map((mode, now(), user_id), |r| {
        Ok(json!({
            "id": r.get::<_, i64>(0)?,
            "thai": r.get::<_, String>(1)?,
            "meanings": meanings_json(r.get(2)?),
            "phonetic": r.get::<_, String>(3)?,
            "box": r.get::<_, i64>(4)?,
        }))
    }).map_err(db_err)?
      .collect::<Result<Vec<_>, _>>().map_err(db_err)?;
    Ok(json_resp(200, json!(words)))
}

fn review(db: &Connection, body: &Value, user_id: i64) -> Result<Resp, String> {
    let word_id = body["word_id"].as_i64().ok_or("word_id required")?;
    let mode = body["mode"].as_str().unwrap_or("");
    let correct = body["correct"].as_bool().ok_or("correct required")?;
    check_mode(mode)?;
    db.query_row("SELECT 1 FROM words WHERE id = ?1 AND user_id = ?2", (word_id, user_id), |_| Ok(()))
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

/// Registration and login share the body shape; both end in a fresh session.
fn account(req: &mut Request, db: &Connection, register: bool) -> Resp {
    let body = body_json(req).unwrap_or(Value::Null);
    let username = body["username"].as_str().unwrap_or("").trim().to_string();
    let password = body["password"].as_str().unwrap_or("");

    if register {
        // ponytail: open registration, no rate limiting — fine for friends &
        // family; the upgrade path is an invite code or a reverse-proxy limit.
        if username.is_empty() || username.chars().count() > 32 {
            return json_resp(400, json!({"error": "username must be 1-32 characters"}));
        }
        if password.chars().count() < 8 {
            return json_resp(400, json!({"error": "password must be at least 8 characters"}));
        }
        let hash = hash_password(password);
        match db.execute(
            "INSERT INTO users(username, password_hash, created_at) VALUES (?1, ?2, ?3)",
            (&username, &hash, now()),
        ) {
            Ok(_) => start_session(db, db.last_insert_rowid(), &username),
            Err(_) => json_resp(400, json!({"error": "username is taken"})),
        }
    } else {
        let found: Option<(i64, String, String)> = db.query_row(
            "SELECT id, username, password_hash FROM users WHERE username = ?1",
            [&username],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).ok();
        match found {
            Some((id, name, hash)) if verify_password(password, &hash) => start_session(db, id, &name),
            _ => json_resp(401, json!({"error": "wrong username or password"})),
        }
    }
}

fn start_session(db: &Connection, user_id: i64, username: &str) -> Resp {
    let token = random_token();
    db.execute(
        "INSERT INTO sessions(token, user_id, created_at) VALUES (?1, ?2, ?3)",
        (&token, user_id, now()),
    ).unwrap();
    let cookie = format!("session={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age=31536000");
    json_resp(200, json!({"username": username}))
        .with_header(Header::from_bytes(&b"Set-Cookie"[..], cookie.as_bytes()).unwrap())
}

fn logout(db: &Connection, token: &str) -> Resp {
    let _ = db.execute("DELETE FROM sessions WHERE token = ?1", [token]);
    json_resp(200, json!({"ok": true})).with_header(
        Header::from_bytes(&b"Set-Cookie"[..], &b"session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0"[..]).unwrap(),
    )
}

/// Returns (user_id, token) for a valid session cookie.
fn session(req: &Request, db: &Connection) -> Option<(i64, String)> {
    let cookie = req.headers().iter().find(|h| h.field.equiv("Cookie"))?;
    let value = cookie.value.as_str().to_string();
    let token = value.split(';').find_map(|c| c.trim().strip_prefix("session="))?.to_string();
    let user_id = db
        .query_row("SELECT user_id FROM sessions WHERE token = ?1", [&token], |r| r.get(0))
        .ok()?;
    Some((user_id, token))
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

/// Extracts and validates word fields. Meanings arrive as a JSON list of
/// non-empty strings (deduplicated case-insensitively) and are stored as JSON.
fn word_fields(body: &Value) -> Result<(String, String, String), String> {
    let thai = body["thai"].as_str().unwrap_or("").trim().to_string();
    let phonetic = body["phonetic"].as_str().unwrap_or("").trim().to_string();
    let mut meanings: Vec<String> = Vec::new();
    for m in body["meanings"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let m = m.as_str().unwrap_or("").trim().to_string();
        if !m.is_empty() && !meanings.iter().any(|e| e.eq_ignore_ascii_case(&m)) {
            meanings.push(m);
        }
    }
    if thai.is_empty() || meanings.is_empty() {
        return Err("thai and at least one meaning are required".into());
    }
    Ok((thai, serde_json::to_string(&meanings).unwrap(), phonetic))
}

/// Stored meanings are JSON; tolerate any stray plain text as a one-item list.
fn meanings_json(raw: String) -> Value {
    serde_json::from_str(&raw).unwrap_or_else(|_| json!([raw]))
}

fn check_mode(mode: &str) -> Result<(), String> {
    if logic::MODES.contains(&mode) { Ok(()) } else { Err(format!("unknown mode '{mode}'")) }
}

fn db_err(e: rusqlite::Error) -> String {
    e.to_string()
}

fn hash_password(password: &str) -> String {
    use argon2::password_hash::{PasswordHasher, SaltString};
    let salt = SaltString::encode_b64(&random_bytes()[..16]).unwrap();
    argon2::Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

fn verify_password(password: &str, hash: &str) -> bool {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};
    PasswordHash::new(hash)
        .is_ok_and(|h| argon2::Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
}

fn random_token() -> String {
    random_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

fn random_bytes() -> [u8; 32] {
    let mut buf = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut buf))
        .expect("read /dev/urandom");
    buf
}
