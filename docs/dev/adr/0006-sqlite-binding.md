# 0006. SQLite binding

- Status: Accepted
- Requirements: PERS-01, PERS-02 (also ARCH-01, ARCH-02, SEC-01)
- Issues: [#34](https://github.com/GuillaumeOj/BIMCraft/issues/34)

## Context

The native project file is a SQLite database ([specification §5.3](../../specifications.md#5-core-model-kernel)). The same core code must read and write it on the desktop and in the browser, where project files live in the Origin Private File System (OPFS), and it must build for `wasm32-unknown-unknown` on every commit (ARCH-01). Core crates may not touch files directly (ARCH-02), and `unsafe` Rust is limited to an audited list (SEC-01).

SQLite does not fit the `platform::Storage` trait well. `Storage` reads and writes whole byte buffers asynchronously; SQLite reads and writes pages synchronously through its virtual file system layer (VFS), which is what gives atomic commits (PERS-02) and partial loading. Routing every page through an async byte store would mean writing a VFS that blocks on futures, which the browser main thread cannot do.

## Decision

### The binding

Rust code talks to SQLite through **[`rusqlite`](https://crates.io/crates/rusqlite)** with its `bundled` feature, version pinned in `[workspace.dependencies]`.

- **Desktop**: `rusqlite` builds the SQLite amalgamation shipped in `libsqlite3-sys` with the platform C compiler. Every user gets the same SQLite version, never the operating system's.
- **Browser** (`wasm32-unknown-unknown`): `rusqlite` switches its low-level layer to [`sqlite-wasm-rs`](https://crates.io/crates/sqlite-wasm-rs), which compiles the same amalgamation for WebAssembly against a small bundled C library. No Emscripten, no JavaScript copy of SQLite; the Rust API is identical on both targets.
- **OPFS**: the browser host opens project databases through the OPFS "sync access handle pool" VFS from [`sqlite-wasm-vfs`](https://crates.io/crates/sqlite-wasm-vfs) (its 0.2 line matches `sqlite-wasm-rs` 0.5). It runs in a dedicated Web Worker, where OPFS offers synchronous file handles, so SQLite stays synchronous and keeps its journal-based atomic commits.

### Where the code lives

A new core crate, **`persist`**, owns what is inside a project file: the schema version (`PRAGMA user_version`, PERS-01), the `application_id` that marks a database as a BIMCraft project, migrations, and transactions. It is listed with the wasm32 crates and the critical crates.

`persist` never opens a file and never chooses a VFS. The host opens a `rusqlite::Connection` on its own storage (a path on desktop, the OPFS VFS in a worker in the browser) and passes it in. The crate therefore stays within ARCH-02 without a new platform trait. `platform::Storage` remains the interface for whole-file data such as side files (PERS-03).

### `unsafe` and native code

BIMCraft code needs no `unsafe`: `rusqlite` wraps the C interface in a safe API, and registering the OPFS VFS is a safe `async` function. The audited list in [quality.md](../quality.md#audited-unsafe-modules) stays empty. The `unsafe` and C code inside `rusqlite`, `libsqlite3-sys` and `sqlite-wasm-rs` is a reviewed dependency like any other (AGENTS.md §4); SQLite itself is among the most heavily tested C libraries.

### Licences

`rusqlite`, `libsqlite3-sys`, `sqlite-wasm-rs` and `sqlite-wasm-vfs` are MIT. SQLite is in the public domain. Their build-time helpers (`cc`, `pkg-config`, `vcpkg`) are MIT or Apache-2.0. All pass `deny.toml`.

## Consequences

- The wasm32 build compiles C, so it needs LLVM: a clang with the WebAssembly backend and its `llvm-ar`. Without `llvm-ar` the build falls back to the system `ar`, which silently drops wasm objects (Apple) or writes no symbol index (GNU): the build passes, but the archive cannot be linked. `.cargo/config.toml` defaults the wasm32 archiver to `llvm-ar`, so a build without it fails instead of passing silently. `scripts/wasm-toolchain.sh` finds both tools (`clang`, then Homebrew LLVM) and `scripts/check-all.sh` uses them; `scripts/setup.sh` reports when they are missing. Desktop builds need the usual platform C compiler.
- The two targets bundle slightly different SQLite releases (3.53.2 on desktop, 3.53.0 in the browser for the pinned versions). The file format is stable across SQLite 3 releases, but upgrades of `rusqlite` and `sqlite-wasm-rs` are reviewed together.
- The browser build of SQLite is single-threaded. In the browser, the core runs project storage inside a dedicated Web Worker; that worker design belongs to the browser host.
- A malformed or hostile project file is handled by SQLite's own checks; `persist` refuses databases that do not carry BIMCraft's `application_id`, and refuses files from newer versions instead of modifying them.
- Errors from SQLite are wrapped without their message, which can quote SQL or values (SEC-03); the underlying error stays available as the source for diagnostics.

## Alternatives considered

| Candidate | Desktop | `wasm32-unknown-unknown` + OPFS | Maintenance | Licence | Verdict |
| --- | --- | --- | --- | --- | --- |
| **`rusqlite` (`bundled`)** | Yes | Yes, through `sqlite-wasm-rs` and `sqlite-wasm-vfs` | Mature, widely used, regular releases | MIT | **Chosen** |
| `sqlx` (SQLite driver) | Yes, async | No wasm32 support | Active | MIT/Apache-2.0 | Fails ARCH-01 |
| `diesel` | Yes | Through the same low-level crates | Active | MIT/Apache-2.0 | An ORM layer the schema generators do not need |
| `turso_core` (pure-Rust SQLite rewrite) | Yes | Yes | Pre-1.0, very frequent releases | MIT | File-format and feature parity not yet proven; revisit later |
| `sql.js` or `wa-sqlite` through JavaScript | No | Yes | Active | MIT | Two code paths; the core cannot use it natively |
| In-memory database saved through `platform::Storage` | Yes | Yes | — | — | Loads whole files into memory: no partial loading, weaker crash safety |

## References

- Specification: §5.3, PERS-01, PERS-02, PERS-03, ARCH-01, ARCH-02, SEC-01, SEC-03.
- Issue: [#34](https://github.com/GuillaumeOj/BIMCraft/issues/34).
- [`rusqlite`](https://github.com/rusqlite/rusqlite), [`sqlite-wasm-rs`](https://github.com/Spxg/sqlite-wasm-rs).
- SQLite documentation: "Atomic Commit In SQLite", "The SQLite OS Interface or VFS", `PRAGMA application_id` and `PRAGMA user_version`.
