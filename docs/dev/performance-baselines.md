# Performance baselines

Reference numbers to compare against when a hot path changes. Timings depend on the machine, so compare runs on the same machine and update this page in the pull request that moves a number. CI does not run the benchmarks ([quality.md](quality.md#benchmarks)); it does gate memory per object.

## Attribute tables (CORE-05)

The model of [ADR 0007](adr/0007-attribute-tables.md), measured with 100,000 wall-like objects, each with a reference, two lengths, a short name, a flag and an integer.

### Time

`cargo bench -p core-model`, release profile. Apple M3, Rust 1.98.1, macOS, 2026-10-14. Median of 10 samples.

| Benchmark | 100,000 objects | Per object |
| --- | --- | --- |
| `store_100k/insert` | 23.4 ms | 234 ns |
| `store_100k/get` (one attribute) | 2.10 ms | 21 ns |
| `store_100k/set` (one attribute) | 2.99 ms | 30 ns |
| `store_100k/remove` | 30.7 ms | 307 ns |

Remove includes returning the removed values. The insert figure was taken while the benchmark still built each object's name inside the timed loop, so it is an upper bound; replace it with the next measurement on an idle machine.

### Memory (NFR-08)

`Store::heap_bytes` after inserting the objects of the `memory_per_object_stays_within_budget` test in `crates/core-model`: **166 bytes per object** on 64-bit targets, about 16.6 MB for the 100,000 objects. Roughly 95 bytes are the values themselves, 28 the spare capacity of growing vectors and 43 the id index. The test fails above a budget of 200 bytes per object; raise it only with a reason in the pull request.
