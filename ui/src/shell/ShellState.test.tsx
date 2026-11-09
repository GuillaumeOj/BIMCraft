import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fakeCore } from "../test-support/fakeCore";
import { renderWithCore } from "../test-support/render";
import { useShellState } from "./ShellState";

function Probe() {
  const { notify, registerModal, modalOpen } = useShellState();
  return (
    <>
      <p>{modalOpen ? "modal" : "no modal"}</p>
      <button type="button" onClick={() => notify("Hello")}>
        Notify
      </button>
      <button type="button" onClick={() => notify("Done it", { undoable: true })}>
        Notify with undo
      </button>
      <button type="button" onClick={() => registerModal()}>
        Open modal
      </button>
    </>
  );
}

const press = (name: string) => act(() => screen.getByRole("button", { name }).click());

afterEach(() => {
  vi.useRealTimers();
});

describe("ShellStateProvider", () => {
  it("counts the modal dialogs that are open", async () => {
    const user = userEvent.setup();
    renderWithCore(<Probe />, fakeCore());
    expect(screen.getByText("no modal")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Open modal" }));
    expect(screen.getByText("modal")).toBeInTheDocument();
  });

  it("shows a plain toast for a few seconds, without Undo", () => {
    renderWithCore(<Probe />, fakeCore());
    vi.useFakeTimers();
    press("Notify");
    expect(screen.getByRole("status")).toHaveTextContent("Hello");
    expect(screen.queryByRole("button", { name: "Undo" })).not.toBeInTheDocument();
    act(() => vi.advanceTimersByTime(7001));
    expect(screen.getByRole("status")).toBeEmptyDOMElement();
  });

  it("offers Undo while the announced step is the last one, and undoes it", async () => {
    const core = fakeCore();
    core.materials.seed("en1992-1-1/C25/30");
    const user = userEvent.setup();
    renderWithCore(<Probe />, core);
    await user.click(screen.getByRole("button", { name: "Notify with undo" }));
    await user.click(await screen.findByRole("button", { name: "Undo" }));
    expect(core.calls.some((call) => call.method === "history.undo")).toBe(true);
    expect(screen.queryByRole("button", { name: "Undo" })).not.toBeInTheDocument();
  });

  it("stops offering Undo once the model has moved on", async () => {
    const core = fakeCore();
    const user = userEvent.setup();
    renderWithCore(<Probe />, core);
    await user.click(screen.getByRole("button", { name: "Notify with undo" }));
    await screen.findByRole("button", { name: "Undo" });
    await waitFor(() =>
      expect(core.calls.some((call) => call.method === "history.state")).toBe(true),
    );
    // Another step is recorded behind the toast's back, then the model announces a change.
    core.materials.handlers["materials.create"]?.({
      id: "a".repeat(32),
      name: "Mix",
      category: "steel",
    });
    await act(async () => core.notifyChange());
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Undo" })).not.toBeInTheDocument(),
    );
    expect(screen.getByRole("status")).toHaveTextContent("Done it");
  });

  it("keeps Undo when a change leaves the history as it was", async () => {
    const core = fakeCore();
    const user = userEvent.setup();
    renderWithCore(<Probe />, core);
    await user.click(screen.getByRole("button", { name: "Notify with undo" }));
    await screen.findByRole("button", { name: "Undo" });
    await act(async () => core.notifyChange());
    await act(async () => core.notifyChange());
    expect(screen.getByRole("button", { name: "Undo" })).toBeInTheDocument();
  });

  it("drops Undo when the history cannot be read", async () => {
    const core = fakeCore();
    core.failNext("history.state", new Error("down"));
    const user = userEvent.setup();
    renderWithCore(<Probe />, core);
    await user.click(screen.getByRole("button", { name: "Notify with undo" }));
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Undo" })).not.toBeInTheDocument(),
    );
    expect(screen.getByRole("status")).toHaveTextContent("Done it");
  });

  it("drops Undo when a later change cannot be checked", async () => {
    const core = fakeCore();
    const user = userEvent.setup();
    renderWithCore(<Probe />, core);
    await user.click(screen.getByRole("button", { name: "Notify with undo" }));
    await screen.findByRole("button", { name: "Undo" });
    core.failNext("history.state", new Error("down"));
    await act(async () => core.notifyChange());
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Undo" })).not.toBeInTheDocument(),
    );
  });

  it("fails outside a provider", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    expect(() => render(<Probe />)).toThrow(
      "useShellState must be used inside <ShellStateProvider>",
    );
  });
});
