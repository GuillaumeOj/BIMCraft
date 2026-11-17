# API contract

No interface is shared between the two tasks: no command API method, IPC message or wasm export changes.

- `front` adds `isModalOpen(): boolean` to the `ShellState` context. It is a UI-internal hook value, used only inside `ui/src/`.
- `back` may change the viewport's frame loop or the Playwright set-up. If it changes an export of the `viewport` wasm module that `ui/src/viewport/` uses, it must say so in its plan, and the lead coordinates with `front`.
