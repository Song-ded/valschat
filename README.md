# messanger

Temporary local room-based messenger in Rust with a pluggable encryption layer.

## Commands

Client:

```powershell
cargo run -- chat --user alice
```

Server:

```powershell
cargo run --bin server
```

By default the server listens on `0.0.0.0:8080`.
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

## Server behavior

- the client encrypts the message locally with the user's `--key`;
- the client sends only ciphertext to the server;
- the server stores only ciphertext;
- the server returns ciphertext to room members;
- each client decrypts locally with its own `--key`;
- the server never needs the plaintext.

## Example message payload

```json
{
  "from": "alice",
  "ciphertext": "4d4b327c..."
}
```

## Current behavior

- rooms are separate from encryption keys;
- users create and join named rooms;
- each room has an owner;
- owner can set member limit, kick users, and ban users;
- when the last member leaves, the room is deleted automatically;
- user must join a room before sending messages;
- the whole key is used for message encryption inside the current room;
- wrong key produces unreadable output;
- local client data is stored in `data`;
- server state is stored in `server-data/state.json`.

## Project layout

- `src/crypto/demo_cipher.rs`: multi-symbol marker-based cipher used on the client
- `src/store.rs`: local file-based client storage
- `src/app.rs`: client room and chat operations
- `src/main.rs`: interactive client terminal UI
- `src/bin/server.rs`: HTTP server for rooms, members, bans, and ciphertext history
