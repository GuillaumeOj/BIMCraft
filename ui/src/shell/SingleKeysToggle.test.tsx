import { screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { fakeCore } from "../test-support/fakeCore";
import { renderWithCore } from "../test-support/render";
import { SINGLE_KEYS_STORAGE_KEY, useShellState } from "./ShellState";
import { SingleKeysToggle } from "./SingleKeysToggle";

function State() {
  const { singleKeys } = useShellState();
  return <output>{singleKeys ? "on" : "off"}</output>;
}

const render = () =>
  renderWithCore(
    <>
      <SingleKeysToggle />
      <State />
    </>,
    fakeCore(),
  ).user;

describe("SingleKeysToggle", () => {
  it("is on by default, and remembers when it is turned off and on again", async () => {
    const user = render();
    const box = screen.getByRole("checkbox", { name: "Single-key shortcuts" });
    expect(box).toBeChecked();
    await user.click(box);
    expect(screen.getByText("off")).toBeInTheDocument();
    expect(localStorage.getItem(SINGLE_KEYS_STORAGE_KEY)).toBe("off");
    await user.click(box);
    expect(screen.getByText("on")).toBeInTheDocument();
    expect(localStorage.getItem(SINGLE_KEYS_STORAGE_KEY)).toBeNull();
  });

  it("starts off when it was turned off before", () => {
    localStorage.setItem(SINGLE_KEYS_STORAGE_KEY, "off");
    render();
    expect(screen.getByRole("checkbox", { name: "Single-key shortcuts" })).not.toBeChecked();
  });

  it("stays on, and works for the session, when the storage cannot be used", async () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    const user = render();
    const box = screen.getByRole("checkbox", { name: "Single-key shortcuts" });
    expect(box).toBeChecked();
    await user.click(box);
    expect(box).not.toBeChecked();
  });
});
