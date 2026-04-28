# messanger

Room-based Rust messenger with client-side encryption and an HTTP server for message delivery.

## Browser client

After deploying the server, open its root URL in a browser:

- [https://valschat.onrender.com/](https://valschat.onrender.com/)

The browser client is served directly by the Rust server, so people can use the chat without installing Rust or running `cargo run`.

Inside the browser client you can:

- register and login
- create and join rooms
- enter your `--key` equivalent in the key field
- send encrypted messages with the same client-side cipher
- decrypt incoming room messages locally in the browser

## Commands

Register:

```powershell
cargo run -- register --user alice --password 1234 --server https://valschat.onrender.com
```

Login:

```powershell
cargo run -- login --user alice --password 1234 --server https://valschat.onrender.com
```

Chat:

```powershell
cargo run -- chat --server https://valschat.onrender.com
```

Logout on this PC:

```powershell
cargo run -- logout --server https://valschat.onrender.com
```

Server from source:

```powershell
cargo run --bin server
```

By default the server listens on `0.0.0.0:25655`.
You can override it with `PORT`.

## Chat commands

```text
--help
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
--logout
--quit
```

## Packaged Windows release

Build ready-to-run `.exe` packages without using `cargo run`:

```powershell
powershell -ExecutionPolicy Bypass -File .\build-release.ps1
```

This creates:

- `dist\client\messanger.exe`
- `dist\client\register.bat`
- `dist\client\login.bat`
- `dist\client\chat.bat`
- `dist\client\status.bat`
- `dist\client\logout.bat`
- `dist\server\server.exe`
- `dist\server\start-server.bat`

Example packaged client usage:

```powershell
dist\client\register.bat alice 1234 https://valschat.onrender.com
dist\client\chat.bat --server https://valschat.onrender.com
```

## Current behavior

- users register and login
- login is remembered on this PC in `client-data/session.json`
- `logout` removes local authorization on this PC
- rooms are separate from encryption keys
- each room has an owner
- owner can set member limit, kick users, and ban users
- when the last member leaves, the room is deleted automatically
- message length is limited to `80` characters
- the client encrypts locally before upload
- the server stores and returns only ciphertext
- ciphertext integrity is checked before normal decryption
- server state is stored in `server-data/state.json`
- server enforces rate limits on auth, room actions, and message sending

## Project layout

- `src/crypto/demo_cipher.rs`: client-side cipher based on the restored C++ algorithm
- `src/store.rs`: blocking HTTP client, session store, and auth API
- `src/app.rs`: client room and chat operations over HTTP
- `src/main.rs`: interactive terminal client
- `src/bin/server.rs`: HTTP server for auth, rooms, members, bans, rate limits, and ciphertext history
- `build-release.ps1`: builds ready-to-run Windows release packages
