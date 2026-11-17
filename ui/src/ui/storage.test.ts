import { describe, expect, it, vi } from "vitest";
import { readStored, writeStored } from "./storage";

describe("readStored / writeStored", () => {
  it("stores, reads and removes a value in local or session storage", () => {
    writeStored("fabrilo.test", "a");
    writeStored("fabrilo.test", "b", "session");
    expect(readStored("fabrilo.test")).toBe("a");
    expect(readStored("fabrilo.test", "session")).toBe("b");
    writeStored("fabrilo.test", null);
    writeStored("fabrilo.test", null, "session");
    expect(readStored("fabrilo.test")).toBeNull();
    expect(readStored("fabrilo.test", "session")).toBeNull();
  });

  it("never throws, and keeps values in memory, when storage cannot be used", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "removeItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(readStored("fabrilo.test")).toBeNull();
    expect(() => writeStored("fabrilo.test", "a")).not.toThrow();
    // Kept in memory instead: the choice lasts for the session.
    expect(readStored("fabrilo.test")).toBe("a");
    expect(readStored("fabrilo.test", "session")).toBeNull();
    writeStored("fabrilo.test", null);
    expect(readStored("fabrilo.test")).toBeNull();
  });

  it("keeps a choice whose write failed even when reading still works", () => {
    writeStored("fabrilo.test", "old");
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("quota");
    });
    vi.spyOn(Storage.prototype, "removeItem").mockImplementation(() => {
      throw new Error("quota");
    });
    writeStored("fabrilo.test", "new");
    expect(readStored("fabrilo.test")).toBe("new");
    writeStored("fabrilo.test", null);
    expect(readStored("fabrilo.test")).toBeNull();
    vi.restoreAllMocks();
    // Storage works again: a successful write replaces what memory kept.
    writeStored("fabrilo.test", "back");
    expect(readStored("fabrilo.test")).toBe("back");
  });
});
