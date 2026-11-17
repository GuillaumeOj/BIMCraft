# API contract

No interface between the tasks changes: no IPC command, event or type is added, renamed or removed. The only shared file is `crates/desktop-app/tauri.conf.json`: `back` edits `productName`, `identifier` and the window `title`; `front` edits the `bundle.icon` entry `icons/Fabrilo.icon`.
