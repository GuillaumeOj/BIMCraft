import { describe, expect, spyOn, test } from "bun:test";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { generate, main, OUTPUT_FILE, SCHEMA_FILE, typeOf } from "./generate-api-types";

const REPO = join(import.meta.dir, "..");

describe("typeOf", () => {
  test("maps the scalar types", () => {
    expect(typeOf({ type: "string", pattern: "^x$" }, "s")).toBe("string");
    expect(typeOf({ type: "integer", minimum: 0 }, "s")).toBe("number");
    expect(typeOf({ type: "number" }, "s")).toBe("number");
    expect(typeOf({ type: "boolean" }, "s")).toBe("boolean");
    expect(typeOf({ type: "null" }, "s")).toBe("null");
  });

  test("maps references, constants and unions", () => {
    expect(typeOf({ $ref: "#/$defs/EntityId" }, "s")).toBe("EntityId");
    expect(typeOf({ const: "1.0.0" }, "s")).toBe('"1.0.0"');
    expect(typeOf({ anyOf: [{ $ref: "#/$defs/A" }, { type: "null" }] }, "s")).toBe("A | null");
    expect(typeOf({ oneOf: [{ const: "ui" }, { const: 2 }] }, "s")).toBe('"ui" | 2');
  });

  test("maps arrays and tuples", () => {
    expect(typeOf({ type: "array", items: { type: "string" }, uniqueItems: true }, "s")).toBe(
      "Array<string>",
    );
    const tuple = { type: "array", prefixItems: [{ type: "string" }, { type: "number" }], items: false, minItems: 2 };
    expect(typeOf(tuple, "s")).toBe("[string, number]");
  });

  test("maps closed objects, with optional members and quoted keys", () => {
    const object = {
      type: "object",
      additionalProperties: false,
      properties: { id: { type: "string" }, "a.b": { type: "null" } },
      required: ["id"],
    };
    expect(typeOf(object, "s")).toBe('{ id: string; "a.b"?: null }');
    expect(typeOf({ type: "object", additionalProperties: false, properties: {} }, "s")).toBe(
      "Record<string, never>",
    );
    expect(typeOf({ type: "object", additionalProperties: false }, "s")).toBe("Record<string, never>");
  });

  test("rejects what it cannot type faithfully, naming where", () => {
    expect(() => typeOf(true, "a")).toThrow("a: expected a schema object");
    expect(() => typeOf({ type: "string", format: "date" }, "a")).toThrow(
      "a: unsupported JSON Schema keyword(s): format",
    );
    expect(() => typeOf({ $ref: "other.json#/x" }, "a")).toThrow("a: unsupported $ref other.json#/x");
    expect(() => typeOf({ type: ["string", "null"] }, "a")).toThrow('a: unsupported type ["string","null"]');
    expect(() => typeOf({ type: "object" }, "a")).toThrow('a: objects must set "additionalProperties": false');
    expect(() => typeOf({ type: "array", prefixItems: [] }, "a")).toThrow('a: tuples must set "items": false');
    expect(() => typeOf({ type: "array" }, "a")).toThrow("a.items: expected a schema object");
  });
});

const method = (name: string, params: unknown, result: unknown) => ({
  description: `Does ${name}.`,
  type: "object",
  properties: { name: { const: name }, version: { const: "1.0.0" }, params, result },
});
const NO_PARAMS = { type: "object", additionalProperties: false, properties: {}, required: [] };

describe("generate", () => {
  test("writes the shared types, then the commands and queries by name", () => {
    const schema = {
      $defs: {
        EntityId: { description: "An id.", type: "string" },
        Plain: { type: "boolean" },
        commands: { "history.undo": method("history.undo", NO_PARAMS, { type: "null" }) },
        queries: { "model.get": method("model.get", NO_PARAMS, { $ref: "#/$defs/EntityId" }) },
        entities: { anything: "is skipped" },
      },
    };
    const generated = generate(schema);
    expect(generated).toContain("/** An id. */\nexport type EntityId = string;\n");
    expect(generated).toContain("\nexport type Plain = boolean;\n");
    expect(generated).toContain(
      '  /** Does history.undo. */\n  "history.undo": { params: Record<string, never>; result: null };\n',
    );
    expect(generated).toContain('export interface Queries {\n  /** Does model.get. */\n  "model.get"');
    expect(generated).not.toContain("anything");
  });

  test("rejects versioned names and malformed schemas", () => {
    const versioned = { $defs: { commands: { "a@1.0.0": method("a", NO_PARAMS, { type: "null" }) }, queries: {} } };
    expect(() => generate(versioned)).toThrow('$defs.a@1.0.0: is named "a"; versioned names are not supported yet');
    expect(() => generate([])).toThrow("schema: expected a schema object");
    expect(() => generate({})).toThrow("$defs: expected a schema object");
  });
});

/** A repository root holding `schema` as the protocol schema and, optionally, `committed`. */
function repo(schema: unknown, committed?: string): string {
  const root = mkdtempSync(join(tmpdir(), "api-types-"));
  mkdirSync(join(root, dirname(SCHEMA_FILE)), { recursive: true });
  mkdirSync(join(root, dirname(OUTPUT_FILE)), { recursive: true });
  writeFileSync(join(root, SCHEMA_FILE), JSON.stringify(schema));
  if (committed !== undefined) writeFileSync(join(root, OUTPUT_FILE), committed);
  return root;
}

const SCHEMA = { $defs: { Id: { type: "string" }, commands: {}, queries: {} } };

describe("main", () => {
  test("writes the file, then finds it up to date", () => {
    const log = spyOn(console, "log").mockImplementation(() => {});
    const root = repo(SCHEMA);
    expect(main([root])).toBe(0);
    expect(readFileSync(join(root, OUTPUT_FILE), "utf8")).toBe(generate(SCHEMA));
    expect(main(["--check", root])).toBe(0);
    expect(log).toHaveBeenLastCalledWith(`${OUTPUT_FILE} is up to date.`);
    log.mockRestore();
  });

  test("fails the check when the file is stale or missing", () => {
    const error = spyOn(console, "error").mockImplementation(() => {});
    expect(main([repo(SCHEMA, "// old\n"), "--check"])).toBe(1);
    expect(main([repo(SCHEMA), "--check"])).toBe(1);
    expect(error).toHaveBeenLastCalledWith(`${OUTPUT_FILE} is stale. Run: bun scripts/generate-api-types.ts`);
    error.mockRestore();
  });

  test("the committed types match the published schema", () => {
    const log = spyOn(console, "log").mockImplementation(() => {});
    const cwd = process.cwd();
    process.chdir(REPO);
    try {
      // No root argument: the current directory, as when run from the repository root.
      expect(main(["--check"])).toBe(0);
    } finally {
      process.chdir(cwd);
    }
    log.mockRestore();
  });
});
