import { act, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiClient, ApiProvider, type Transport } from "../api";
import { I18nProvider } from "../i18n/I18nProvider";
import { fakeCore } from "../test-support/fakeCore";
import { renderWithCore } from "../test-support/render";
import { GeorefProvider, useGeoref } from "./GeorefContext";

function Probe() {
  const { state, dialogOpen, notify } = useGeoref();
  return (
    <>
      <p>{`${state.status}${state.status === "ready" ? (state.info === null ? ":none" : ":placed") : ""}${state.status === "ready" && state.reloadError ? ":reload-failed" : ""}`}</p>
      <p>{dialogOpen ? "open" : "closed"}</p>
      <button type="button" onClick={() => notify("Hello")}>
        Notify
      </button>
      <button type="button" onClick={() => notify("Second")}>
        Notify second
      </button>
    </>
  );
}

afterEach(() => {
  vi.useRealTimers();
});

describe("GeorefProvider", () => {
  it("reads the georeference and reads it again after each model.changed", async () => {
    const core = fakeCore();
    renderWithCore(<Probe />, core);
    expect(screen.getByText("loading")).toBeInTheDocument();
    expect(await screen.findByText("ready:none")).toBeInTheDocument();
    core.replace({
      crs_epsg: 2154,
      base_point: [0, 0, 0],
      grid_rotation: 0,
      scale: 1,
      survey_point: null,
    });
    await act(async () => core.notifyChange());
    expect(await screen.findByText("ready:placed")).toBeInTheDocument();
  });

  it("keeps the newest answer when an older one arrives late", async () => {
    const requests: { id: number; answer: (georeference: unknown) => void }[] = [];
    let push: (message: string) => void = () => {};
    const transport: Transport = {
      send: (message) =>
        new Promise((resolve) => {
          const { id } = JSON.parse(message);
          const answer = (georeference: unknown) =>
            resolve(JSON.stringify({ jsonrpc: "2.0", id, result: { georeference } }));
          requests.push({ id, answer });
        }),
      listen: async (onMessage) => {
        push = onMessage;
        return () => {};
      },
    };
    render(
      <I18nProvider>
        <ApiProvider client={new ApiClient(transport)}>
          <GeorefProvider>
            <Probe />
          </GeorefProvider>
        </ApiProvider>
      </I18nProvider>,
    );
    await waitFor(() => expect(requests).toHaveLength(1));
    push(JSON.stringify({ jsonrpc: "2.0", method: "model.changed", params: {} }));
    await waitFor(() => expect(requests).toHaveLength(2));
    await act(async () => requests[1]?.answer({ site: "s" }));
    expect(await screen.findByText("ready:placed")).toBeInTheDocument();
    await act(async () => requests[0]?.answer(null));
    expect(screen.getByText("ready:placed")).toBeInTheDocument();
  });

  it("opens the dialog with Ctrl+Shift+L or Cmd+Shift+L and ignores other keys", async () => {
    renderWithCore(<Probe />, fakeCore());
    await screen.findByText("ready:none");
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "l", ctrlKey: true }));
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "l", ctrlKey: true, shiftKey: true, altKey: true }),
    );
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "x", ctrlKey: true, shiftKey: true }));
    expect(screen.getByText("closed")).toBeInTheDocument();
    await act(async () => {
      window.dispatchEvent(
        new KeyboardEvent("keydown", { key: "L", metaKey: true, shiftKey: true }),
      );
    });
    expect(screen.getByText("open")).toBeInTheDocument();
  });

  it("keeps the last georeference when a reload fails, and clears the error on the next success", async () => {
    const core = fakeCore({
      georeference: {
        crs_epsg: 2154,
        base_point: [0, 0, 0],
        grid_rotation: 0,
        scale: 1,
        survey_point: null,
      },
    });
    renderWithCore(<Probe />, core);
    await screen.findByText("ready:placed");
    core.failNext("georef.get", new Error("down"));
    await act(async () => core.notifyChange());
    expect(await screen.findByText("ready:placed:reload-failed")).toBeInTheDocument();
    await act(async () => core.notifyChange());
    expect(await screen.findByText("ready:placed")).toBeInTheDocument();
  });

  it("shows a toast for a few seconds, and a new one replaces it", async () => {
    renderWithCore(<Probe />, fakeCore());
    await screen.findByText("ready:none");
    vi.useFakeTimers();
    act(() => screen.getByRole("button", { name: "Notify" }).click());
    expect(screen.getByRole("status")).toHaveTextContent("Hello");
    act(() => screen.getByRole("button", { name: "Notify second" }).click());
    expect(screen.getByRole("status")).toHaveTextContent("Second");
    act(() => vi.advanceTimersByTime(5001));
    expect(screen.getByRole("status")).toBeEmptyDOMElement();
  });

  it("goes on when the core cannot push notifications", async () => {
    const transport: Transport = {
      send: async (message) =>
        JSON.stringify({
          jsonrpc: "2.0",
          id: JSON.parse(message).id,
          result: { georeference: null },
        }),
      listen: () => Promise.reject(new Error("no events")),
    };
    const { unmount } = render(
      <I18nProvider>
        <ApiProvider client={new ApiClient(transport)}>
          <GeorefProvider>
            <Probe />
          </GeorefProvider>
        </ApiProvider>
      </I18nProvider>,
    );
    await waitFor(() => expect(screen.getByText("ready:none")).toBeInTheDocument());
    unmount();
  });
});

describe("useGeoref", () => {
  it("fails outside a provider", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    expect(() => render(<Probe />)).toThrow("useGeoref must be used inside <GeorefProvider>");
  });
});
