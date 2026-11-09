import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeCore } from "../test-support/fakeCore";
import { renderWithCore } from "../test-support/render";
import { Dialog } from "./Dialog";

function setup() {
  return renderWithCore(
    <Dialog titleId="t" onCancel={() => undefined}>
      <h2 id="t">Title</h2>
      <button type="button">First</button>
      <div role="option" aria-selected="false" tabIndex={-1}>
        Row
      </div>
      <button type="button">Middle</button>
      <button type="button">Last</button>
    </Dialog>,
    fakeCore(),
  ).user;
}

describe("Dialog", () => {
  it("lets Tab continue from a control outside the tab order, and wraps at the ends", async () => {
    const user = setup();
    screen.getByRole("option", { name: "Row" }).focus();
    await user.tab();
    expect(screen.getByRole("button", { name: "Middle" })).toHaveFocus();
    await user.tab();
    await user.tab();
    expect(screen.getByRole("button", { name: "First" })).toHaveFocus();
    await user.tab({ shift: true });
    expect(screen.getByRole("button", { name: "Last" })).toHaveFocus();
  });
});
