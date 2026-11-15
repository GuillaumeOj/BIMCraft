import { afterEach, describe, expect, it, vi } from "vitest";
import {
  AUTO_CHECK_STORAGE_KEY,
  CHANNEL_STORAGE_KEY,
  readAutoCheck,
  readChannel,
  writeAutoCheck,
  writeChannel,
} from "./settings";

afterEach(() => localStorage.clear());

describe("update settings", () => {
  it("default to Stable and automatic checks", () => {
    expect(readChannel()).toBe("stable");
    expect(readAutoCheck()).toBe(true);
  });

  it("remember the channel and the switch", () => {
    writeChannel("beta");
    writeAutoCheck(false);
    expect(localStorage.getItem(CHANNEL_STORAGE_KEY)).toBe("beta");
    expect(localStorage.getItem(AUTO_CHECK_STORAGE_KEY)).toBe("off");
    expect(readChannel()).toBe("beta");
    expect(readAutoCheck()).toBe(false);
    writeAutoCheck(true);
    expect(readAutoCheck()).toBe(true);
  });

  it("ignores a stored value that is not a channel", () => {
    localStorage.setItem(CHANNEL_STORAGE_KEY, "nightly");
    expect(readChannel()).toBe("stable");
  });

  it("works for the session when storage throws", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(readChannel()).toBe("stable");
    expect(readAutoCheck()).toBe(true);
    expect(() => writeChannel("beta")).not.toThrow();
    expect(() => writeAutoCheck(false)).not.toThrow();
  });
});
