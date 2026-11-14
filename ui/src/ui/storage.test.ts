import { describe, expect, it, vi } from "vitest";
import { readStored, writeStored } from "./storage";

describe("readStored / writeStored", () => {
  it("stores, reads and removes a value in local or session storage", () => {
    writeStored("bimcraft.test", "a");
    writeStored("bimcraft.test", "b", "session");
    expect(readStored("bimcraft.test")).toBe("a");
    expect(readStored("bimcraft.test", "session")).toBe("b");
    writeStored("bimcraft.test", null);
    writeStored("bimcraft.test", null, "session");
    expect(readStored("bimcraft.test")).toBeNull();
    expect(readStored("bimcraft.test", "session")).toBeNull();
  });

  it("never throws when storage cannot be used", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(readStored("bimcraft.test")).toBeNull();
    expect(() => writeStored("bimcraft.test", "a")).not.toThrow();
  });
});
