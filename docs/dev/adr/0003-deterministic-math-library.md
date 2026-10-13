# 0003. Deterministic math library

- Status: Accepted
- Requirements: ENG-04 (also ARCH-01, GEO-07)
- Issues: [#9](https://github.com/GuillaumeOj/BIMCraft/issues/9), epic [#8](https://github.com/GuillaumeOj/BIMCraft/issues/8)

## Context

ENG-04 requires geometry to give the same bits on every target: Windows, macOS and Linux desktops and the browser (`wasm32-unknown-unknown`, ARCH-01). Server-ordered collaboration replays operations on several machines and depends on it.

Basic IEEE 754 operations (`+ − × ÷`, `sqrt`, fused multiply-add) are correctly rounded, so every conforming target gives the same result. Rust never contracts a multiply and an add into a fused one on its own, so code built from these operations is deterministic.

Transcendental functions are not. The std methods `f64::sin`, `f64::exp`, `f64::powf`, `f64::powi` and their relatives call the platform's C maths library or an LLVM intrinsic. Their documentation, under "Unspecified precision", states that their precision "is non-deterministic": it may differ between platforms and Rust versions. `to_degrees` and `to_radians` carry the same warning. The macOS, glibc, Windows and wasm32 implementations do differ in the last bit for some inputs. One differing bit in a rotation angle or an arc point moves a constructed vertex and can change a later boolean.

Nothing in the core uses these methods yet. This record picks one implementation before the first use and makes the build reject the std methods.

## Decision

### The library

Core crates (`scripts/wasm-crates.txt`) compute every transcendental function with the **[`libm`](https://crates.io/crates/libm) crate**, version pinned in `[workspace.dependencies]`. They call it directly (`libm::sin(x)`, `libm::atan2(y, x)`); there is no wrapper crate.

`libm` is a pure-Rust port of musl's maths library, compiled from one source. Its only hardware code paths (default `arch` feature) are for exactly rounded operations such as `sqrt`, `fma` and `floor`, so it produces the same bits on every supported target, given the same `libm` version. Its x87 paths apply only to 32-bit x86 without SSE2, which BIMCraft does not target.

### Banned std methods

`clippy.toml` bans, for both `f64` and `f32`, every stable method the std documentation marks "Unspecified precision" (`sin`, `exp`, `powf`, `powi`, `to_degrees` and so on), except the deprecated `abs_sub`, which the deprecation lint already rejects. `clippy.toml` holds the list, with the `libm` replacement in each reason. Angle conversions multiply by the constant explicitly (`x * (180.0 / PI)`), a single correctly rounded multiplication.

The ban rides on the ARCH-02 `forbid` of `clippy::disallowed_methods` ([quality.md](../quality.md#architecture-rules)), so no core crate can opt out. Clippy has a single lint for both rules: a crate or integration test that opts out of ARCH-02 also opts out of this ban. Exact powers of two use `libm::scalbn(1.0, k)`.

### Allowed std methods

These stay allowed because IEEE 754 defines their result exactly: `sqrt`, `mul_add`, `abs`, `floor`, `ceil`, `round`, `trunc`, `fract`, `copysign`, `rem_euclid`, `div_euclid`.

`min`, `max` and `clamp` stay allowed too, with one documented exception: given `+0.0` and `−0.0` they may return either. The values compare equal, so geometric decisions are unaffected, but the stored bits can differ. Where bits are hashed, serialised or compared, normalise `−0.0` to `+0.0` first. Likewise, the bit pattern of a NaN produced by arithmetic is unspecified; the kernel rejects NaN input, so none should be stored.

## Consequences

- Results are reproducible across targets for a given `libm` version. Upgrading `libm` can change last bits, so an upgrade is a deliberate change reviewed like a change to `LINEAR_TOLERANCE` ([ADR 0001](0001-geometry-tolerance-policy.md)), and the cross-target comparison ([#10](https://github.com/GuillaumeOj/BIMCraft/issues/10), a separate task) must pass on it.
- `libm` functions are accurate to within about one unit in the last place, not correctly rounded. That is enough for geometry, where decisions use exact predicates and constructed points are rounded anyway.
- `libm` may be slower than the platform library on desktop. No hot path uses transcendentals yet; benchmarks will show if one does.
- The lint sees only our own code. A dependency added to a core crate must not call the std methods itself; for example, `num-traits` must use its `libm` feature, not `std`. Reviewing a new core dependency (AGENTS.md §4) includes this check.

## Alternatives considered

| Candidate | Accuracy | Maintenance | Licence | Pure Rust, wasm32 | Verdict |
| --- | --- | --- | --- | --- | --- |
| **`libm`** | About 1 ulp (musl) | rust-lang project, used by `compiler-builtins` for Rust's own targets without a libm | MIT | Yes, `no_std` | **Chosen** |
| std methods | Platform-dependent | Rust project | MIT/Apache-2.0 | Calls platform code | Breaks ENG-04 |
| `micromath` | Fast approximations, errors far above 1 ulp | Small community crate | MIT/Apache-2.0 | Yes, but `f32` only | Too inaccurate for `f64` geometry (GEO-07) |
| CORE-MATH or CRlibm bindings | Correctly rounded | Active research projects | MIT / LGPL | No: C code, needs a C toolchain for wasm32 | Breaks the pure-Rust, single-toolchain build |
| `num-traits` (`libm` feature), `core_maths` | Same as `libm` | Community crates | MIT/Apache-2.0 | Yes | Only wrappers over `libm`; one more dependency for method syntax |
| Our own implementation | Our choice | Us | — | Yes | Large effort and risk for no gain today |

A correctly rounded pure-Rust library would make results independent of the library version too. None is mature today; a later record can switch if one appears.

## References

- Specification: ENG-04, ARCH-01, GEO-07.
- Issues: [#8](https://github.com/GuillaumeOj/BIMCraft/issues/8), [#9](https://github.com/GuillaumeOj/BIMCraft/issues/9), [#10](https://github.com/GuillaumeOj/BIMCraft/issues/10).
- Rust standard library documentation, `f64::sin` and related methods ("Unspecified precision"), `f64::max` and `f64::min` (signed zeros).
- [rust-lang/compiler-builtins](https://github.com/rust-lang/compiler-builtins), home of the `libm` crate.
