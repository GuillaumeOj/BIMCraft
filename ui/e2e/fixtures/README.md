# Scene fixtures

Binary scene deltas (`scene-wire` version 1, api-contract.md section 1) for the UI tests and the e2e tests. A Rust test writes them from known documents and fails when they are stale: `cargo test -p host-desktop --test scene_fixtures`, and `UPDATE_SNAPSHOTS=1` regenerates them.

| File | What it holds |
| --- | --- |
| `scene-empty.bin` | The full scene of an empty document: revision 0, no strings, no mesh, no element. |
| `scene-blocks.bin` | The full scene of three blocks, revision 3. Blocks `…01` and `…02` are 4 × 2 × 3 m at (0, 0, 0) and (6, 0, 0): they share one mesh. Block `…03` is 2 × 2 × 6 m at (0, 6, 0), turned by a quarter turn: a second mesh. Category `Block`, discipline `core`. |
| `scene-move.bin` | The delta from revision 3 to 4 after moving block `…02` by (0, 3, 0): one element record at (6, 3, 0), no mesh. Apply it on top of `scene-blocks.bin`. |
