#!/usr/bin/env bun
// Checks update packages against the updater public key before a release uploads them (ENG-05),
// the way the installed program's updater will: a minisign signature (Ed25519, over the BLAKE2b-512
// hash of the file), whose trusted comment must carry the release's version, because tauri.conf.json
// sets `requireSignedVersion`. The Tauri CLI can sign but not verify.
//
// Usage: UPDATER_PUBLIC_KEY=… VERSION=1.2.0 bun scripts/release/verify-signature.ts <package>...
//   Each package's signature is read from <package>.sig.

import { createHash, createPublicKey, verify } from "node:crypto";
import { readFileSync } from "node:fs";

const KEY_ID_LENGTH = 8;
const PUBLIC_KEY_LENGTH = 32;
const SIGNATURE_LENGTH = 64;
const TRUSTED_COMMENT = "trusted comment: ";

/** Text that Tauri stores in base64: the key or signature file minisign writes. */
const decodeText = (base64: string, what: string): string[] => {
  const text = Buffer.from(base64.trim(), "base64").toString("utf8");
  const lines = text.split("\n").map((line) => line.replace(/\r$/, ""));
  if (lines.length < 2) throw new Error(`the ${what} is not a minisign ${what}`);
  return lines;
};

const decodeBytes = (base64: string | undefined, length: number, what: string): Buffer => {
  const bytes = Buffer.from(base64 ?? "", "base64");
  if (bytes.length !== length) throw new Error(`the ${what} is not a minisign ${what}`);
  return bytes;
};

type PublicKey = { id: Buffer; key: ReturnType<typeof createPublicKey> };

function parsePublicKey(base64: string): PublicKey {
  const bytes = decodeBytes(decodeText(base64, "public key")[1], 2 + KEY_ID_LENGTH + PUBLIC_KEY_LENGTH, "public key");
  if (bytes.toString("latin1", 0, 2) !== "Ed") throw new Error("the public key is not an Ed25519 minisign key");
  const raw = bytes.subarray(2 + KEY_ID_LENGTH);
  const key = createPublicKey({ format: "jwk", key: { kty: "OKP", crv: "Ed25519", x: raw.toString("base64url") } });
  return { id: bytes.subarray(2, 2 + KEY_ID_LENGTH), key };
}

type Signature = { prehashed: boolean; id: Buffer; signature: Buffer; trustedComment: string; global: Buffer };

function parseSignature(base64: string): Signature {
  const lines = decodeText(base64, "signature");
  const bytes = decodeBytes(lines[1], 2 + KEY_ID_LENGTH + SIGNATURE_LENGTH, "signature");
  const algorithm = bytes.toString("latin1", 0, 2);
  if (algorithm !== "ED" && algorithm !== "Ed") throw new Error("the signature is not an Ed25519 minisign signature");
  const comment = lines[2] ?? "";
  if (!comment.startsWith(TRUSTED_COMMENT)) throw new Error("the signature has no trusted comment");
  return {
    prehashed: algorithm === "ED",
    id: bytes.subarray(2, 2 + KEY_ID_LENGTH),
    signature: bytes.subarray(2 + KEY_ID_LENGTH),
    trustedComment: comment.slice(TRUSTED_COMMENT.length),
    global: decodeBytes(lines[3], SIGNATURE_LENGTH, "signature"),
  };
}

/** The `version` field of a trusted comment (tab-separated `key:value` pairs), if any. */
export const signedVersion = (trustedComment: string): string | null =>
  trustedComment
    .split("\t")
    .find((field) => field.startsWith("version:"))
    ?.slice("version:".length) ?? null;

/**
 * Checks that `signature` (the base64 content of a `.sig` file) signs `data` with `publicKey` (the
 * base64 public key Tauri embeds) and was made for `version`; throws why otherwise.
 */
export function verifySignature(data: Uint8Array, signature: string, publicKey: string, version: string): void {
  const key = parsePublicKey(publicKey);
  const sig = parseSignature(signature);
  if (!sig.id.equals(key.id)) throw new Error("the signature was made with another key");
  const signed = sig.prehashed ? createHash("blake2b512").update(data).digest() : data;
  if (!verify(null, signed, key.key, sig.signature)) throw new Error("the signature does not match the file");
  const comment = Buffer.concat([sig.signature, Buffer.from(sig.trustedComment, "utf8")]);
  if (!verify(null, comment, key.key, sig.global)) throw new Error("the trusted comment was altered");
  const signedFor = signedVersion(sig.trustedComment);
  if (signedFor === null) throw new Error("the signature carries no version: sign with --app-version");
  if (signedFor.replace(/^v/, "") !== version.replace(/^v/, "")) {
    throw new Error(`the signature is for version ${signedFor}, not ${version}`);
  }
}

type Env = Record<string, string | undefined>;

export function main(files: string[], env: Env): number {
  const { UPDATER_PUBLIC_KEY: publicKey, VERSION: version } = env;
  if (!publicKey || !version || files.length === 0) {
    console.error("usage: UPDATER_PUBLIC_KEY=… VERSION=… bun scripts/release/verify-signature.ts <package>...");
    return 2;
  }
  let failures = 0;
  for (const file of files) {
    try {
      verifySignature(readFileSync(file), readFileSync(`${file}.sig`, "utf8"), publicKey, version);
      console.log(`${file}: signed for ${version} with the updater key`);
    } catch (error) {
      console.error(`${file}: ${(error as Error).message}`);
      failures += 1;
    }
  }
  return failures === 0 ? 0 : 1;
}

if (import.meta.main) process.exit(main(process.argv.slice(2), process.env));
