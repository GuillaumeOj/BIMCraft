import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ActionMenu } from "./ActionMenu";

function setup() {
  const first = vi.fn();
  const second = vi.fn();
  const user = userEvent.setup();
  render(
    <ActionMenu
      label="Actions for X"
      items={[
        { label: "First", onSelect: first },
        { label: "Second", onSelect: second },
      ]}
    >
      More
    </ActionMenu>,
  );
  return { user, first, second, button: screen.getByRole("button", { name: "Actions for X" }) };
}

describe("ActionMenu", () => {
  it("opens on click with the first item focused, and runs an item then closes", async () => {
    const { user, first, button } = setup();
    expect(button).toHaveAttribute("aria-haspopup", "menu");
    expect(button).toHaveAttribute("aria-expanded", "false");
    await user.click(button);
    expect(screen.getByRole("menuitem", { name: "First" })).toHaveFocus();
    await user.click(screen.getByRole("menuitem", { name: "First" }));
    expect(first).toHaveBeenCalledOnce();
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("moves between items with the arrows, wrapping round", async () => {
    const { user, button } = setup();
    await user.click(button);
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "Second" })).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "First" })).toHaveFocus();
    await user.keyboard("{ArrowUp}");
    expect(screen.getByRole("menuitem", { name: "Second" })).toHaveFocus();
    await user.keyboard("a");
    expect(screen.getByRole("menuitem", { name: "Second" })).toHaveFocus();
  });

  it("closes with Escape and gives focus back to the button; a closed menu ignores keys", async () => {
    const { user, button } = setup();
    button.focus();
    await user.keyboard("{ArrowDown}{Escape}");
    await user.click(button);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(button).toHaveFocus();
  });

  it("closes when the button is pressed again", async () => {
    const { user, button } = setup();
    await user.click(button);
    await user.click(button);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });
});
