# Thai Practice

A small self-hosted web app for practicing Thai vocabulary: add words, then
drill them in five modes with Leitner spaced repetition. Works on laptop and
phone (installable via the PWA manifest), and ships an optional on-screen
Kedmanee Thai keyboard for people without a Thai keyboard, plus an on-screen
IPA keyboard (Chulalongkorn CTFL system: vowels incl. ɛ ɔ ə ɯ, consonants
incl. ʔ ŋ, and combining tone marks) for typing transliterations.

**Modes:** Spelling (see English, type Thai) · Reading (read Thai, self-grade
the meaning) · Translation (read Thai, type English) · Pronunciation (read
Thai, type the phonetic — only words with a phonetic are drilled) · Listening
(hear Thai via the browser's built-in text-to-speech, type it).

A word can have several meanings, added one by one in the form. In Translation
mode you must name every meaning: each guess is checked exactly against a whole
meaning ("go" does not count for "go to"), a wrong guess ends the round, and
the reveal flags each meaning as found or missed.

Words you miss fall back to Leitner box 1 and come around again immediately;
words you know climb boxes 1→5 with review intervals of 0, 1, 3, 7 and 21 days,
per word *and* per mode.

Multi-user: anyone can register an account (username + password, argon2-hashed);
each account has its own words and progress.

## Run

Needs Rust. The binary embeds SQLite (`rusqlite` bundled), so there is nothing
else to install.

```sh
cargo run
```

Then open http://localhost:3000 and create an account.

Configuration (env vars):

| Var        | Default    | Meaning                        |
|------------|------------|--------------------------------|
| `PORT`     | `3000`     | Listen port                    |
| `DB_PATH`  | `./thai.db`| SQLite database file           |

## Test

```sh
cargo test
```

Covers the Leitner scheduling and a full API round-trip (register, login,
auth wall, user isolation, add word, quiz, review, box movement, logout)
against the real binary.

## Deploy to a VPS

```sh
cargo build --release
# copy target/release/thai-practice and the public/ directory to the server,
# run from the directory that contains public/:
PORT=3000 ./thai-practice
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
User=thai
WorkingDirectory=/opt/thai-practice
ExecStart=/opt/thai-practice/thai-practice
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

Back up by copying `thai.db`.

## Notes

- The frontend loads Vue from a CDN, so the app needs internet access
  (the data server is remote anyway).
- Registration is open — anyone who can reach the server can create an
  account. Put an invite code or reverse-proxy rate limit in front if that
  ever becomes a problem.
- Upgrading from the single-password version: words carried `user_id = 1`,
  so the **first account registered** on the upgraded server owns them.
  Register your own account before sharing the URL.
- Deferred on purpose: learn-the-keyboard mode, speech recognition, offline
  mode/service worker.
