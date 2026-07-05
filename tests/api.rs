// API round-trip against the real binary: register/login, add a word, quiz,
// review, Leitner box movement, user isolation, and the 401 wall. Raw
// TcpStream keeps us free of an HTTP-client dependency.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command};

struct Server {
    child: Child,
    port: u16,
    db: std::path::PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.db);
    }
}

fn start_server() -> Server {
    // ponytail: freeing a port by binding-then-dropping is racy, but fine for a local test.
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let db = std::env::temp_dir().join(format!("thai-test-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&db);
    let child = Command::new(env!("CARGO_BIN_EXE_thai-practice"))
        .env("PORT", port.to_string())
        .env("DB_PATH", &db)
        .spawn()
        .unwrap();
    let server = Server { child, port, db };
    for _ in 0..100 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return server;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("server did not come up");
}

/// Minimal HTTP/1.1 request; returns (status, headers, body).
fn http(port: u16, method: &str, path: &str, cookie: &str, body: &str) -> (u32, String, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let cookie_line = if cookie.is_empty() { String::new() } else { format!("Cookie: {cookie}\r\n") };
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{cookie_line}Content-Length: {}\r\n\r\n{body}",
        body.len()
    ).unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap();
    let status: u32 = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, head.to_string(), body.to_string())
}

fn session_cookie(head: &str) -> String {
    head.lines()
        .find_map(|l| l.strip_prefix("Set-Cookie: "))
        .and_then(|c| c.split(';').next())
        .expect("session cookie set")
        .to_string()
}

#[test]
fn api_round_trip() {
    let server = start_server();
    let p = server.port;

    // no cookie -> 401
    assert_eq!(http(p, "GET", "/api/words", "", "").0, 401);

    // registration validation
    assert_eq!(http(p, "POST", "/api/register", "", r#"{"username":" ","password":"longenough"}"#).0, 400);
    assert_eq!(http(p, "POST", "/api/register", "", r#"{"username":"alice","password":"short"}"#).0, 400);

    // register -> auto-login cookie
    let (status, head, body) = http(p, "POST", "/api/register", "", r#"{"username":"alice","password":"correcthorse"}"#);
    assert_eq!(status, 200);
    assert!(body.contains("alice"), "got: {body}");
    session_cookie(&head);

    // duplicate username rejected, case-insensitively
    assert_eq!(http(p, "POST", "/api/register", "", r#"{"username":"Alice","password":"correcthorse"}"#).0, 400);

    // login
    assert_eq!(http(p, "POST", "/api/login", "", r#"{"username":"alice","password":"wrong"}"#).0, 401);
    let (status, head, _) = http(p, "POST", "/api/login", "", r#"{"username":"alice","password":"correcthorse"}"#);
    assert_eq!(status, 200);
    let cookie = session_cookie(&head);

    // garbage cookie is still 401
    assert_eq!(http(p, "GET", "/api/words", "session=deadbeef", "").0, 401);

    // whoami
    let (_, _, body) = http(p, "GET", "/api/me", &cookie, "");
    assert!(body.contains("alice"), "got: {body}");

    // validation: empty thai rejected
    let (status, _, _) = http(p, "POST", "/api/words", &cookie, r#"{"thai":" ","meaning":"water"}"#);
    assert_eq!(status, 400);

    // add a word
    let (status, _, body) = http(p, "POST", "/api/words", &cookie,
        r#"{"thai":"น้ำ","meaning":"water","phonetic":"nám"}"#);
    assert_eq!(status, 200);
    assert!(body.contains("\"id\""), "got: {body}");

    // it is due in the spell quiz
    let (status, _, body) = http(p, "GET", "/api/quiz?mode=spell", &cookie, "");
    assert_eq!(status, 200);
    assert!(body.contains("น้ำ"), "got: {body}");

    // correct review -> box 2, no longer due
    let (status, _, body) = http(p, "POST", "/api/review", &cookie,
        r#"{"word_id":1,"mode":"spell","correct":true}"#);
    assert_eq!(status, 200);
    assert!(body.contains("\"box\":2"), "got: {body}");
    let (_, _, body) = http(p, "GET", "/api/quiz?mode=spell", &cookie, "");
    assert_eq!(body.trim(), "[]");

    // other modes are unaffected: still due for reading
    let (_, _, body) = http(p, "GET", "/api/quiz?mode=read", &cookie, "");
    assert!(body.contains("น้ำ"), "got: {body}");

    // wrong review -> back to box 1 and immediately due again
    let (_, _, body) = http(p, "POST", "/api/review", &cookie,
        r#"{"word_id":1,"mode":"spell","correct":false}"#);
    assert!(body.contains("\"box\":1"), "got: {body}");
    let (_, _, body) = http(p, "GET", "/api/quiz?mode=spell", &cookie, "");
    assert!(body.contains("น้ำ"), "got: {body}");

    // words list reports per-mode boxes
    let (_, _, body) = http(p, "GET", "/api/words", &cookie, "");
    assert!(body.contains(r#""spell":1"#), "got: {body}");

    // phonetic mode only quizzes words that have a phonetic
    let (status, _, _) = http(p, "POST", "/api/words", &cookie, r#"{"thai":"ไป","meaning":"go"}"#);
    assert_eq!(status, 200);
    let (_, _, body) = http(p, "GET", "/api/quiz?mode=phonetic", &cookie, "");
    assert!(body.contains("น้ำ") && !body.contains("ไป"), "got: {body}");

    // unknown mode rejected
    assert_eq!(http(p, "GET", "/api/quiz?mode=hack", &cookie, "").0, 400);

    // user isolation: bob sees nothing of alice's and cannot touch it
    let (_, head, _) = http(p, "POST", "/api/register", "", r#"{"username":"bob","password":"correcthorse"}"#);
    let bob = session_cookie(&head);
    let (_, _, body) = http(p, "GET", "/api/words", &bob, "");
    assert_eq!(body.trim(), "[]");
    let (_, _, body) = http(p, "GET", "/api/quiz?mode=spell", &bob, "");
    assert_eq!(body.trim(), "[]");
    assert_eq!(http(p, "PUT", "/api/words/1", &bob, r#"{"thai":"x","meaning":"y"}"#).0, 400);
    assert_eq!(http(p, "DELETE", "/api/words/1", &bob, "").0, 400);
    assert_eq!(http(p, "POST", "/api/review", &bob, r#"{"word_id":1,"mode":"spell","correct":true}"#).0, 400);

    // logout invalidates the session
    assert_eq!(http(p, "POST", "/api/logout", &cookie, "").0, 200);
    assert_eq!(http(p, "GET", "/api/words", &cookie, "").0, 401);

    // path traversal blocked
    assert_eq!(http(p, "GET", "/../Cargo.toml", "", "").0, 404);
}
