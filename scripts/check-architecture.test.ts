import { describe, expect, spyOn, test } from "bun:test";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import {
  type Crate,
  checkExtensionSurface,
  checkFacade,
  checkForbid,
  checkLayering,
  checkRepo,
  layersOf,
  loadCrates,
  localDependencies,
  main,
  parseCrateList,
  parseLayers,
  parseSurface,
} from "./check-architecture";

const LAYERS = parseLayers("# comment\nplatform\n\ncore-model geom # kernel\napi\nsolver-fem\n");
const FORBID = "#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]\n";

const toml = (content: string) => Bun.TOML.parse(content) as Record<string, unknown>;

// What the root [workspace.dependencies] would declare for the crates used below.
const WORKSPACE = toml(`
platform = { path = "crates/platform" }
geom = { path = "crates/geom" }
core-model = { path = "crates/core-model" }
api = { path = "crates/api" }
stray = { path = "crates/stray" }
geometry = { package = "geom", path = "crates/geom" }
core-ext = { path = "crates/core-ext" }
disc-arch = { path = "crates/disc-arch" }
thiserror = "=2.0.21"
`);

const crate = (name: string, deps = ""): Crate => ({
  name,
  dir: `crates/${name}`,
  manifest: toml(`[package]\nname = "${name}"\n${deps}`),
});

function repo(files: Record<string, string>): string {
  const root = mkdtempSync(join(tmpdir(), "arch-"));
  const all = {
    "Cargo.toml": '[workspace]\n[workspace.dependencies]\ncore-model = { path = "crates/core-model" }\n',
    "scripts/crate-layers.txt": "platform\ncore-model\nhost\n",
    "scripts/wasm-crates.txt": "platform\ncore-model\n",
    "scripts/extension-surface.txt": "# No crate yet.\n",
    ...files,
  };
  for (const [path, content] of Object.entries(all)) {
    mkdirSync(join(root, dirname(path)), { recursive: true });
    writeFileSync(join(root, path), content);
  }
  return root;
}

describe("parseLayers and parseCrateList", () => {
  test("ignore comments and blank lines", () => {
    expect(LAYERS).toEqual([["platform"], ["core-model", "geom"], ["api"], ["solver-fem"]]);
    expect(parseCrateList(" a\n\nc\n")).toEqual(new Set(["a", "c"]));
  });
});

describe("layersOf", () => {
  test("matches exact names only", () => {
    expect(layersOf(LAYERS, "platform")).toEqual([0]);
    expect(layersOf(LAYERS, "geom")).toEqual([1]);
    expect(layersOf(LAYERS, "solver-fem")).toEqual([3]);
    expect(layersOf(LAYERS, "solver")).toEqual([]);
  });

  test("reports every layer that declares a crate", () => {
    expect(layersOf(parseLayers("solver-fem\nsolver-fem\n"), "solver-fem")).toEqual([0, 1]);
  });
});

describe("localDependencies", () => {
  test("reads every section and target, following renames, and skips registry crates", () => {
    const { manifest } = crate(
      "x",
      `
[dependencies]
platform = { workspace = true }
thiserror = { workspace = true }
serde = "=1.0.228"
[dev-dependencies]
geometry = { workspace = true }
[build-dependencies]
my-api = { package = "api", path = "../api" }
[target.'cfg(windows)'.dependencies]
core-model = { workspace = true }
[target.'cfg(unix)'.dev-dependencies]
stray = { path = "../stray" }
`,
    );
    expect(localDependencies(manifest, WORKSPACE).sort()).toEqual([
      "api",
      "core-model",
      "geom",
      "platform",
      "stray",
    ]);
  });

  test("ignores a workspace dependency missing from the root", () => {
    expect(localDependencies(crate("x", "[dependencies]\nghost = { workspace = true }\n").manifest, {})).toEqual(
      [],
    );
  });
});

describe("checkLayering", () => {
  test("accepts dependencies on the same or lower layers", () => {
    const crates = [
      crate("platform", "[dependencies]\nthiserror = { workspace = true }\n"),
      crate("geom", "[dependencies]\nplatform = { workspace = true }\n"),
      crate("core-model", "[dependencies]\ngeom = { workspace = true }\n"),
      crate("solver-fem", "[dependencies]\napi = { workspace = true }\n"),
      crate("api", "[dependencies]\ncore-model = { workspace = true }\n"),
    ];
    expect(checkLayering(crates, LAYERS, WORKSPACE)).toEqual([]);
  });

  test("ignores a registry crate that shares a member's name", () => {
    const crates = [crate("api"), crate("platform", '[dependencies]\napi = "=0.1.0"\n')];
    expect(checkLayering(crates, LAYERS, WORKSPACE)).toEqual([]);
  });

  test("rejects an upward dependency in any section, through renames", () => {
    const crates = [
      crate("api"),
      crate("core-model", '[dev-dependencies]\nmy-api = { package = "api", path = "../api" }\n'),
      crate("platform", "[target.'cfg(unix)'.build-dependencies]\ngeometry = { workspace = true }\n"),
      crate("geom"),
    ];
    expect(checkLayering(crates, LAYERS, WORKSPACE)).toEqual([
      {
        file: "crates/core-model/Cargo.toml",
        message: "core-model (layer 1) depends on api (layer 2), a higher layer; see ARCH-04",
      },
      {
        file: "crates/platform/Cargo.toml",
        message: "platform (layer 0) depends on geom (layer 1), a higher layer; see ARCH-04",
      },
    ]);
  });

  test("requires every crate to sit in exactly one layer", () => {
    const layers = parseLayers("solver-fem\nsolver-fem\napi\n");
    const crates = [crate("stray"), crate("solver-fem"), crate("api", "[dependencies]\nstray = { workspace = true }\n")];
    expect(checkLayering(crates, layers, WORKSPACE).map((i) => i.message)).toEqual([
      'crate "stray" has no layer; fix scripts/crate-layers.txt',
      'crate "solver-fem" is in several layers (0, 1); fix scripts/crate-layers.txt',
    ]);
  });
});

describe("checkForbid", () => {
  test("accepts crate roots that forbid both lints, in any order or among other lints", () => {
    const root = repo({
      "crates/core-model/src/lib.rs": `//! Docs.\n\n${FORBID}`,
      "crates/core-model/src/main.rs":
        "#![forbid(unsafe_code)]\n#![forbid( clippy::disallowed_types,clippy::disallowed_methods )]\n",
    });
    expect(checkForbid(root, crate("core-model"))).toEqual([]);
  });

  test("rejects a missing or partial forbid and a crate-level clippy config", () => {
    const root = repo({
      "crates/core-model/src/lib.rs": "#![forbid(clippy::disallowed_methods)]\n",
      "crates/core-model/src/main.rs": "#![deny(clippy::disallowed_methods, clippy::disallowed_types)]\n",
      "crates/core-model/clippy.toml": "",
      "crates/core-model/.clippy.toml": "",
    });
    expect(checkForbid(root, crate("core-model")).map((i) => i.file)).toEqual([
      "crates/core-model/clippy.toml",
      "crates/core-model/.clippy.toml",
      "crates/core-model/src/lib.rs",
      "crates/core-model/src/main.rs",
    ]);
  });

  test("rejects a crate without a root it can check", () => {
    expect(checkForbid(repo({}), crate("core-model"))).toEqual([
      { file: "crates/core-model", message: "core crate has no src/lib.rs or src/main.rs to check" },
    ]);
  });
});

describe("checkExtensionSurface", () => {
  const SURFACE = parseSurface("# comment\ncore-ext\nplatform # traits\n\n");
  const listed = [crate("core-ext"), crate("platform")];
  const check = (...crates: Crate[]) => checkExtensionSurface([...listed, ...crates], SURFACE, WORKSPACE);

  test("parses the list, ignoring comments and blank lines", () => {
    expect(SURFACE).toEqual(new Set(["core-ext", "platform"]));
  });

  test("accepts a discipline that depends only on the listed crates and registry crates", () => {
    const disc = crate(
      "disc-arch",
      `
[dependencies]
core-ext = { workspace = true }
thiserror = { workspace = true }
[dev-dependencies]
platform = { workspace = true }
`,
    );
    expect(check(disc)).toEqual([]);
  });

  test("rejects any other core crate in every section and target, through renames", () => {
    const disc = crate(
      "disc-struct",
      `
[dependencies]
core-ext = { workspace = true }
core-model = { workspace = true }
[dev-dependencies]
my-api = { package = "api", path = "../api" }
[build-dependencies]
stray = { path = "../stray" }
[target.'cfg(unix)'.dependencies]
geometry = { workspace = true }
`,
    );
    expect(check(disc)).toEqual(
      ["core-model", "api", "stray", "geom"].map((dep) => ({
        file: "crates/disc-struct/Cargo.toml",
        message: `disc-struct depends on ${dep}, which is not a public extension interface; use core-ext (ARCH-03)`,
      })),
    );
  });

  test("reports a dependency declared in several sections once", () => {
    const disc = crate(
      "disc-mep",
      "[dependencies]\ngeom = { workspace = true }\n[dev-dependencies]\ngeometry = { workspace = true }\n",
    );
    expect(check(disc).map((i) => i.message)).toEqual([
      "disc-mep depends on geom, which is not a public extension interface; use core-ext (ARCH-03)",
    ]);
  });

  test("rejects a discipline depending on another one, even a listed one", () => {
    const surface = new Set([...SURFACE, "disc-arch"]);
    const crates = [...listed, crate("disc-arch"), crate("disc-mep", "[dependencies]\ndisc-arch = { workspace = true }\n")];
    expect(checkExtensionSurface(crates, surface, WORKSPACE)).toEqual([
      {
        file: "crates/disc-mep/Cargo.toml",
        message: "disc-mep depends on disc-arch, another discipline; disciplines share code only through core-ext (ARCH-03)",
      },
    ]);
  });

  test("leaves the dependencies of other crates to the layering check", () => {
    expect(check(crate("api", "[dependencies]\ncore-model = { workspace = true }\n"))).toEqual([]);
  });

  test("requires every listed crate to exist", () => {
    expect(checkExtensionSurface([crate("platform")], SURFACE, WORKSPACE)).toEqual([
      {
        file: "scripts/extension-surface.txt",
        message: '"core-ext" is not a crate in crates/; fix the list (ARCH-03)',
      },
    ]);
  });
});

describe("checkFacade", () => {
  const facade = (lib: string) => checkFacade(repo({ "crates/core-ext/src/lib.rs": lib }), crate("core-ext"));
  const message = (what: string) =>
    `core-ext re-exports ${what}; name each item of the extension surface instead (ARCH-03)`;

  test("accepts items re-exported by name, whatever the comments say", () => {
    const lib = `//! Never \`pub use core_model::*;\`.
${FORBID}
/* pub use geom; */
pub mod model {
    pub use ::core_model::{EntityId, Model}; // not pub use x::*;
    pub use core_model::units::{Length as Len, Quantity};
    pub use self::inner::Thing;
    pub(crate) use geom::*;
    mod inner { pub struct Thing; }
}
`;
    expect(facade(lib)).toEqual([]);
  });

  test("rejects a glob re-export, at any depth", () => {
    for (const lib of ["pub use core_model::*;", "pub mod m {\n    pub use ::geom::{Point2, path::*};\n}"]) {
      expect(facade(lib)).toEqual([{ file: "crates/core-ext/src/lib.rs", message: message("a glob") }]);
    }
  });

  test("rejects a whole-crate re-export in any form", () => {
    for (const lib of [
      "pub use core_model;",
      "pub use ::geom as geometry;",
      "pub use {core_model, geom};",
      "pub use core_model::{self, EntityId};",
      "pub extern crate geom;",
    ]) {
      expect(facade(lib).map((i) => i.message)).toEqual([message("a whole crate")]);
    }
  });

  test("reports each kind of leak once", () => {
    expect(facade("pub use a;\npub use b;\npub use c::*;\n").map((i) => i.message)).toEqual([
      message("a whole crate"),
      message("a glob"),
    ]);
  });

  test("rejects a façade without a crate root", () => {
    expect(checkFacade(repo({}), crate("core-ext"))).toEqual([
      { file: "crates/core-ext", message: "core-ext has no src/lib.rs to check" },
    ]);
  });
});

describe("loadCrates", () => {
  test("loads crates and reports unreadable manifests", () => {
    const root = repo({
      "crates/platform/Cargo.toml": '[package]\nname = "platform"\n',
      "crates/broken/Cargo.toml": "[lib]\n",
      "crates/garbled/Cargo.toml": "[package\n",
      "crates/notes/README.md": "not a crate",
    });
    const { crates, issues } = loadCrates(root);
    expect(crates.map((c) => c.name)).toEqual(["platform"]);
    expect(issues.map((i) => [i.file, i.message.split(":")[0]])).toEqual([
      ["crates/broken/Cargo.toml", "manifest has no [package] name"],
      ["crates/garbled/Cargo.toml", "cannot parse"],
    ]);
  });
});

describe("checkRepo", () => {
  test("checks layering for every crate and the forbid only in core crates", () => {
    const root = repo({
      "crates/platform/Cargo.toml":
        '[package]\nname = "platform"\n[dependencies]\ncore-model = { workspace = true }\n',
      "crates/platform/src/lib.rs": FORBID,
      "crates/core-model/Cargo.toml": '[package]\nname = "core-model"\n',
      "crates/core-model/src/lib.rs": "#![allow(clippy::style)]\n",
      "crates/host/Cargo.toml": '[package]\nname = "host"\n',
      "crates/host/src/main.rs": "#![allow(clippy::disallowed_methods)]\n",
    });
    expect(checkRepo(root).map((i) => i.file)).toEqual(["crates/platform/Cargo.toml", "crates/core-model/src/lib.rs"]);
  });

  test("reads the root workspace without dependencies", () => {
    const root = repo({
      "Cargo.toml": "[workspace]\n",
      "crates/platform/Cargo.toml": '[package]\nname = "platform"\n',
      "crates/platform/src/lib.rs": FORBID,
    });
    expect(checkRepo(root)).toEqual([]);
  });

  test("passes a repository without crates", () => {
    expect(checkRepo(repo({}))).toEqual([]);
  });

  test("checks the extension surface of disciplines and the façade's re-exports", () => {
    const root = repo({
      "Cargo.toml": `[workspace]\n[workspace.dependencies]\ncore-model = { path = "crates/core-model" }\ncore-ext = { path = "crates/core-ext" }\n`,
      "scripts/crate-layers.txt": "platform\ncore-model\ncore-ext\ndisc-arch\n",
      "scripts/wasm-crates.txt": "",
      "scripts/extension-surface.txt": "core-ext\nplatform\n",
      "crates/core-model/Cargo.toml": '[package]\nname = "core-model"\n',
      "crates/core-ext/Cargo.toml": '[package]\nname = "core-ext"\n[dependencies]\ncore-model = { workspace = true }\n',
      "crates/core-ext/src/lib.rs": "pub use core_model::*;\n",
      "crates/disc-arch/Cargo.toml":
        '[package]\nname = "disc-arch"\n[dependencies]\ncore-ext = { workspace = true }\ncore-model = { workspace = true }\n',
    });
    expect(checkRepo(root).map((i) => i.message)).toEqual([
      '"platform" is not a crate in crates/; fix the list (ARCH-03)',
      "disc-arch depends on core-model, which is not a public extension interface; use core-ext (ARCH-03)",
      "core-ext re-exports a glob; name each item of the extension surface instead (ARCH-03)",
    ]);
  });

  test("passes on this repository", () => {
    expect(checkRepo(join(import.meta.dir, ".."))).toEqual([]);
  });
});

describe("main", () => {
  test("returns 0 and reports success", () => {
    const log = spyOn(console, "log").mockImplementation(() => undefined);
    expect(main(repo({}))).toBe(0);
    expect(log).toHaveBeenCalledTimes(1);
    log.mockRestore();
  });

  test("returns 1 and lists every issue", () => {
    const root = repo({ "crates/stray/Cargo.toml": '[package]\nname = "stray"\n' });
    const error = spyOn(console, "error").mockImplementation(() => undefined);
    expect(main(root)).toBe(1);
    expect(error).toHaveBeenCalledTimes(2);
    error.mockRestore();
  });
});
