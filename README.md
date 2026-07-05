# Thai Practice

A small self-hosted web app for practicing Thai vocabulary: add words, then
drill them in four modes with Leitner spaced repetition. Works on laptop and
phone (installable via the PWA manifest), and ships an optional on-screen
Kedmanee Thai keyboard for people without a Thai keyboard.

**Modes:** Spelling (see English, type Thai) · Reading (read Thai, self-grade
the meaning) · Translation (read Thai, type English) · Listening (hear Thai
via the browser's built-in text-to-speech, type it).

Words you miss fall back to Leitner box 1 and come around again immediately;
words you know climb boxes 1→5 with review intervals of 0, 1, 3, 7 and 21 days,
per word *and* per mode.

## Run

Needs Rust. The binary embeds SQLite (`rusqlite` bundled), so there is nothing
else to install.

```sh
PASSWORD=choose-a-password cargo run
```

Then open http://localhost:3000.

Configuration (env vars):

| Var        | Default    | Meaning                        |
|------------|------------|--------------------------------|
| `PASSWORD` | *required* | Login password                 |
| `PORT`     | `3000`     | Listen port                    |
| `DB_PATH`  | `./thai.db`| SQLite database file           |

## Test

```sh
cargo test
```

Covers the Leitner scheduling and a full API round-trip (login, auth wall,
add word, quiz, review, box movement) against the real binary.

## Deploy to a VPS

```sh
cargo build --release
# copy target/release/thai-practice and the public/ directory to the server,
# run from the directory that contains public/:
PASSWORD=... PORT=3000 ./thai-practice
```

Put it behind your reverse proxy for HTTPS (the session cookie is the only
secret in transit), e.g. Caddy:

```
thai.example.com {
    reverse_proxy localhost:3000
}
```

A minimal systemd unit:

```ini
[Unit]
Description=Thai Practice
After=network.target

[Service]
WorkingDirectory=/opt/thai-practice
Environment=PASSWORD=choose-a-password
ExecStart=/opt/thai-practice/thai-practice
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

Back up by copying `thai.db`.

## Notes

- The frontend loads Vue from a CDN, so the app needs internet access
  (the data server is remote anyway).
- The words table is keyed by `user_id` (currently always 1) so real user
  accounts can be added later without a data migration.
- Deferred on purpose: learn-the-keyboard mode, speech recognition, offline
  mode/service worker.
