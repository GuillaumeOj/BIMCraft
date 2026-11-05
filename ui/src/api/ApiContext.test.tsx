import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ApiProvider, useApi } from "./ApiContext";
import type { ApiClient } from "./client";

function Probe() {
  return <p>{useApi() === client ? "same" : "other"}</p>;
}
const client = {} as ApiClient;

describe("useApi", () => {
  it("returns the client of the provider", () => {
    render(
      <ApiProvider client={client}>
        <Probe />
      </ApiProvider>,
    );
    expect(screen.getByText("same")).toBeInTheDocument();
  });

  it("fails outside a provider", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    expect(() => render(<Probe />)).toThrow("useApi must be used inside <ApiProvider>");
  });
});
