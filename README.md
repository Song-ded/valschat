# messanger

Room-based Rust messenger with client-side encryption and an HTTP server for message delivery.

## Commands

Client:

```powershell
cargo run -- chat --user alice --server http://127.0.0.1:25655
```

Server:

```powershell
cargo run --bin server
```

By default the server listens on `0.0.0.0:25655`.
You can override it with `PORT`.

## Chat commands

```text
--key <secret>
--create-room <name> [limit]
--join-room <name>
--leave-room
--rooms
--members
--set-limit <number>
--kick <user>
--ban <user>
--refresh
--quit
```

## Server API

```text
GET  /health
GET  /rooms
POST /rooms
POST /rooms/{room}/join
POST /rooms/{room}/leave
POST /rooms/{room}/limit
POST /rooms/{room}/kick
POST /rooms/{room}/ban
GET  /rooms/{room}/members
GET  /rooms/{room}/messages?after_id=<id>
POST /rooms/{room}/messages
```

## Message flow

- client encrypts the message locally with the active `--key`
- client sends only ciphertext to the server
- server stores only ciphertext and returns only ciphertext
- every client decrypts locally with its own `--key`

## Example message payload

```json
{
  "from": "alice",
  "ciphertext": "4d4b327c..."
}
```

## Current behavior

- rooms are separate from encryption keys
- users create and join named rooms
- each room has an owner
- owner can set member limit, kick users, and ban users
- when the last member leaves, the room is deleted automatically
- user must join a room before sending messages
- the whole key is used for message encryption inside the current room
- wrong key is shown as `[wrong key] ...`
- server state is stored in `server-data/state.json`

## Project layout

- `src/crypto/demo_cipher.rs`: client-side cipher based on the restored C++ algorithm
- `src/store.rs`: blocking HTTP client for rooms and ciphertext history
- `src/app.rs`: client room and chat operations over HTTP
- `src/main.rs`: interactive terminal client
- `src/bin/server.rs`: HTTP server for rooms, members, bans, and ciphertext history
