import { describe, expect, it } from "vitest";
import {
  channelLocked,
  failureKind,
  formatBytes,
  initialUpdateState,
  type Phase,
  percentDownloaded,
  pillKind,
  retryOf,
  type UpdateState,
  updateReducer,
} from "./updateState";

const offer = { version: "0.4.0", date: null, notes: "" };
const at = (phase: Phase, extra: Partial<UpdateState> = {}): UpdateState => ({
  ...initialUpdateState,
  phase,
  offer,
  ...extra,
});

describe("updateReducer", () => {
  it("checks: started, then up to date or available", () => {
    const checking = updateReducer(initialUpdateState, { type: "checkStarted" });
    expect(checking.phase).toBe("checking");
    expect(updateReducer(checking, { type: "checked", offer: null, at: 5 })).toMatchObject({
      phase: "upToDate",
      offer: null,
      checkedAt: 5,
    });
    expect(updateReducer(checking, { type: "checked", offer, at: 6 })).toMatchObject({
      phase: "available",
      offer,
      checkedAt: 6,
    });
  });

  it("a new check clears the failure and the dismissal", () => {
    const state = at("available", { failure: "network", dismissed: true });
    expect(updateReducer(state, { type: "checkStarted" })).toMatchObject({
      failure: null,
      dismissed: false,
    });
  });

  it("a failed check goes back to idle: shown when asked for, silent at start-up", () => {
    const checking = at("checking");
    expect(
      updateReducer(checking, { type: "checkFailed", code: "network", silent: false }),
    ).toMatchObject({
      phase: "idle",
      offer: null,
      failure: "network",
    });
    expect(
      updateReducer(checking, { type: "checkFailed", code: "network", silent: true }).failure,
    ).toBeNull();
  });

  it("downloads: progress, then ready or back to available with the failure", () => {
    const downloading = updateReducer(at("available"), { type: "downloadStarted" });
    expect(downloading).toMatchObject({ phase: "downloading", progress: null });
    const progress = { downloaded: 1, total: 2 };
    const moving = updateReducer(downloading, { type: "progress", progress });
    expect(moving.progress).toEqual(progress);
    expect(updateReducer(moving, { type: "downloaded" })).toMatchObject({
      phase: "ready",
      progress: null,
    });
    expect(updateReducer(moving, { type: "downloadFailed", code: "signature" })).toMatchObject({
      phase: "available",
      progress: null,
      failure: "signature",
    });
  });

  it("restart: confirm, cancel, install", () => {
    const confirm = updateReducer(at("ready", { failure: "install" }), { type: "confirm" });
    expect(confirm).toMatchObject({ phase: "confirm", failure: null });
    expect(updateReducer(confirm, { type: "cancelConfirm" }).phase).toBe("ready");
    expect(updateReducer(confirm, { type: "installStarted" }).phase).toBe("restarting");
  });

  it("an install failure keeps the package, except when the host lost it", () => {
    const restarting = at("restarting");
    expect(updateReducer(restarting, { type: "installFailed", code: "install" })).toMatchObject({
      phase: "ready",
      failure: "install",
    });
    expect(
      updateReducer(restarting, { type: "installFailed", code: "not_downloaded" }),
    ).toMatchObject({ phase: "available", failure: "not_downloaded" });
  });

  it("later dismisses the offer", () => {
    expect(updateReducer(at("available"), { type: "later" }).dismissed).toBe(true);
  });
});

describe("failureKind", () => {
  it.each([
    ["network", "network"],
    ["signature", "signature"],
    ["manifest", "manifest"],
    ["install", "install"],
    ["not_downloaded", "install"],
    ["no_update", "other"],
    ["unavailable", "other"],
    ["unknown", "other"],
  ] as const)("%s is worded as %s", (code, kind) => {
    expect(failureKind(code)).toBe(kind);
  });
});

describe("retryOf", () => {
  it("retries what failed", () => {
    expect(retryOf(at("idle", { failure: "network" }))).toBe("check");
    expect(retryOf(at("idle"))).toBe("check");
    expect(retryOf(at("available", { failure: "signature" }))).toBe("download");
    expect(retryOf(at("available", { failure: "not_downloaded" }))).toBe("download");
    expect(retryOf(at("ready", { failure: "install" }))).toBe("confirm");
    expect(retryOf(at("available", { failure: "no_update" }))).toBe("check");
    expect(retryOf(at("ready", { failure: "unavailable" }))).toBe("check");
    expect(retryOf(at("upToDate", { failure: "network" }))).toBe("check");
  });
});

describe("pillKind", () => {
  it.each([
    [at("idle"), "idle"],
    [at("checking"), "idle"],
    [at("upToDate"), "idle"],
    [at("available"), "available"],
    [at("available", { dismissed: true }), "idle"],
    [at("available", { failure: "network" }), "failed"],
    [at("downloading"), "downloading"],
    [at("ready"), "ready"],
    [at("ready", { dismissed: true }), "ready"],
    [at("confirm"), "ready"],
    [at("restarting"), "ready"],
  ] as const)("shows the right entry for %#", (state, kind) => {
    expect(pillKind(state)).toBe(kind);
  });
});

describe("percentDownloaded", () => {
  it("floors, caps and needs a known total", () => {
    expect(percentDownloaded(null)).toBeNull();
    expect(percentDownloaded({ downloaded: 5, total: null })).toBeNull();
    expect(percentDownloaded({ downloaded: 5, total: 0 })).toBeNull();
    expect(percentDownloaded({ downloaded: 199, total: 1000 })).toBe(19);
    expect(percentDownloaded({ downloaded: 1500, total: 1000 })).toBe(100);
  });
});

describe("channelLocked", () => {
  it("locks while a package could be discarded", () => {
    for (const phase of ["checking", "downloading", "ready", "confirm", "restarting"] as const) {
      expect(channelLocked(at(phase))).toBe(true);
    }
    for (const phase of ["idle", "upToDate", "available"] as const) {
      expect(channelLocked(at(phase))).toBe(false);
    }
  });
});

describe("formatBytes", () => {
  const units = ["B", "kB", "MB", "GB"] as const;
  it("uses decimal units and one decimal below 10", () => {
    expect(formatBytes(0, units, "en")).toBe("0 B");
    expect(formatBytes(999, units, "en")).toBe("999 B");
    expect(formatBytes(2_400_000, units, "en")).toBe("2.4 MB");
    expect(formatBytes(38_400_000, units, "en")).toBe("38 MB");
    expect(formatBytes(1_500, units, "en")).toBe("1.5 kB");
    expect(formatBytes(5_000_000_000_000, units, "en")).toBe("5,000 GB");
  });

  it("follows the locale and treats negatives as zero", () => {
    expect(formatBytes(2_400_000, ["o", "ko", "Mo", "Go"], "fr")).toBe("2,4 Mo");
    expect(formatBytes(-5, units, "en")).toBe("0 B");
  });
});
