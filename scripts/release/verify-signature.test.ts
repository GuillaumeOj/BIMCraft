import { describe, expect, spyOn, test } from "bun:test";
import { generateKeyPairSync, sign } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { main, signedVersion, verifySignature } from "./verify-signature";

// Made with `cargo tauri signer` of the pinned Tauri CLI: a throw-away key pair (the private key is
// not kept), package.bin signed with --app-version 1.2.0-beta.1, and unversioned.bin.sig, a
// signature of the same bytes without a version.
const FIXTURES = join(import.meta.dir, "fixtures");
const fixture = (name: string) => readFileSync(join(FIXTURES, name), "utf8");
const PACKAGE = readFileSync(join(FIXTURES, "package.bin"));
const SIGNATURE = fixture("package.bin.sig");
const KEY = fixture("updater.key.pub");
const VERSION = "1.2.0-beta.1";

const b64 = (text: string) => Buffer.from(text).toString("base64");
const lines = (base64: string) => Buffer.from(base64, "base64").toString("utf8").split("\n");
const withLine = (base64: string, index: number, line: string) => {
  const all = lines(base64);
  all[index] = line;
  return b64(all.join("\n"));
};

describe("verifySignature", () => {
  test("accepts a package signed by the Tauri CLI for its version", () => {
    expect(() => verifySignature(PACKAGE, SIGNATURE, KEY, VERSION)).not.toThrow();
    expect(() => verifySignature(PACKAGE, `${SIGNATURE}\n`, KEY, `v${VERSION}`)).not.toThrow();
  });

  test("refuses another file, another key or another version", () => {
    expect(() => verifySignature(Buffer.from("tampered"), SIGNATURE, KEY, VERSION)).toThrow("does not match the file");
    expect(() => verifySignature(PACKAGE, SIGNATURE, fixture("other.key.pub"), VERSION)).toThrow("another key");
    expect(() => verifySignature(PACKAGE, SIGNATURE, KEY, "1.2.0")).toThrow("for version 1.2.0-beta.1, not 1.2.0");
  });

  test("refuses a signature without a version, as requireSignedVersion does", () => {
    expect(() => verifySignature(PACKAGE, fixture("unversioned.bin.sig"), KEY, VERSION)).toThrow("carries no version");
  });

  test("refuses an altered trusted comment", () => {
    const comment = lines(SIGNATURE)[2] ?? "";
    const forged = withLine(SIGNATURE, 2, comment.replace(VERSION, "9.9.9"));
    expect(() => verifySignature(PACKAGE, forged, KEY, "9.9.9")).toThrow("trusted comment was altered");
  });

  test("refuses malformed keys and signatures", () => {
    expect(() => verifySignature(PACKAGE, SIGNATURE, b64("one line"), VERSION)).toThrow("not a minisign public key");
    expect(() => verifySignature(PACKAGE, SIGNATURE, withLine(KEY, 1, "c2hvcnQ="), VERSION)).toThrow("not a minisign public key");
    const rsa = Buffer.from(lines(KEY)[1] ?? "", "base64");
    rsa.write("RS", 0, "latin1");
    expect(() => verifySignature(PACKAGE, SIGNATURE, withLine(KEY, 1, rsa.toString("base64")), VERSION)).toThrow("not an Ed25519 minisign key");
    expect(() => verifySignature(PACKAGE, withLine(SIGNATURE, 1, "c2hvcnQ="), KEY, VERSION)).toThrow("not a minisign signature");
    const other = Buffer.from(lines(SIGNATURE)[1] ?? "", "base64");
    other.write("XX", 0, "latin1");
    expect(() => verifySignature(PACKAGE, withLine(SIGNATURE, 1, other.toString("base64")), KEY, VERSION)).toThrow("not an Ed25519 minisign signature");
    expect(() => verifySignature(PACKAGE, withLine(SIGNATURE, 2, "untrusted"), KEY, VERSION)).toThrow("no trusted comment");
    expect(() => verifySignature(PACKAGE, withLine(SIGNATURE, 3, ""), KEY, VERSION)).toThrow("not a minisign signature");
  });

  test("checks the legacy, unhashed signatures too", () => {
    // minisign's "Ed" algorithm signs the bytes themselves; the key file format is the same.
    const { publicKey, privateKey } = generateKeyPairSync("ed25519");
    const id = Buffer.from("01234567");
    const raw = Buffer.from(publicKey.export({ format: "jwk" }).x ?? "", "base64url");
    const key = b64(`untrusted comment: test\n${Buffer.concat([Buffer.from("Ed"), id, raw]).toString("base64")}\n`);
    const signature = sign(null, PACKAGE, privateKey);
    const comment = `timestamp:0\tfile:package.bin\tversion:${VERSION}`;
    const global = sign(null, Buffer.concat([signature, Buffer.from(comment)]), privateKey);
    const sig = b64(
      `untrusted comment: test\n${Buffer.concat([Buffer.from("Ed"), id, signature]).toString("base64")}\ntrusted comment: ${comment}\n${global.toString("base64")}\n`,
    );
    expect(() => verifySignature(PACKAGE, sig, key, VERSION)).not.toThrow();
  });
});

describe("signedVersion", () => {
  test("reads the version field of a trusted comment", () => {
    expect(signedVersion("timestamp:1\tfile:a\tversion:1.0.0")).toBe("1.0.0");
    expect(signedVersion("timestamp:1\tfile:a")).toBeNull();
  });
});

describe("main", () => {
  const env = { UPDATER_PUBLIC_KEY: KEY, VERSION };

  test("checks every package and reports each failure", () => {
    const log = spyOn(console, "log").mockImplementation(() => {});
    const error = spyOn(console, "error").mockImplementation(() => {});
    const file = join(FIXTURES, "package.bin");
    expect(main([file], env)).toBe(0);
    expect(log).toHaveBeenCalledWith(`${file}: signed for ${VERSION} with the updater key`);
    expect(main([file, join(FIXTURES, "missing.bin")], env)).toBe(1);
    expect(error).toHaveBeenCalledTimes(1);
    expect(main([file], { ...env, VERSION: "2.0.0" })).toBe(1);
    log.mockRestore();
    error.mockRestore();
  });

  test("needs the key, the version and a package", () => {
    const error = spyOn(console, "error").mockImplementation(() => {});
    expect(main([], env)).toBe(2);
    expect(main(["x"], { VERSION })).toBe(2);
    expect(main(["x"], { UPDATER_PUBLIC_KEY: KEY })).toBe(2);
    error.mockRestore();
  });
});
