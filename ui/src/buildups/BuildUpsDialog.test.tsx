import { act, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ApiError } from "../api";
import { fakeCore, refusal } from "../test-support/fakeCore";
import { renderWithCore } from "../test-support/render";
import { BuildUpsDialog } from "./BuildUpsDialog";

const C25 = "en1992-1-1/C25/30";
type Core = ReturnType<typeof fakeCore>;

const AIR = "a".repeat(32);

/** A core with concrete C25/30 and a generic material "Air" in the project. */
function projectCore() {
  const core = fakeCore();
  core.materials.seed(C25);
  core.materials.handlers["materials.create"]?.({ id: AIR, name: "Air", category: "generic" });
  return core;
}

function open(core: Core = projectCore(), onClose = vi.fn()) {
  const view = renderWithCore(<BuildUpsDialog onClose={onClose} />, core);
  return { ...view, core, onClose };
}

async function concreteId(core: Core) {
  const items = (await core.client.query("materials.list", {})).items;
  return items.find((item) => item.name === "C25/30")?.id ?? "";
}

async function withWall(core: Core = projectCore()) {
  const material = await concreteId(core);
  core.buildUps.seed("Wall A", "wall", [
    { material, thickness: 0.013, function: "finish" },
    { material, thickness: 0.2, function: "structure" },
  ]);
  return core;
}

/** A row's main button: its name starts with `name` (the menu buttons start with "Actions"). */
const row = (name: RegExp) => screen.findByRole("button", { name: new RegExp(`^${name.source}`) });
const field = (name: string) => screen.findByRole("textbox", { name });
const calls = (core: Core, method: string) => core.calls.filter((call) => call.method === method);
const select = (name: string) => screen.getByRole("combobox", { name });

describe("the list", () => {
  it("explains an empty project and offers both new build-ups, the first focused", async () => {
    const { user } = open();
    expect(await screen.findByText(/No build-ups yet/)).toBeInTheDocument();
    const wall = screen.getByRole("button", { name: "New wall build-up" });
    expect(wall).toHaveFocus();
    expect(screen.getByRole("button", { name: "New slab build-up" })).toBeInTheDocument();
    await user.click(wall);
    expect(await screen.findByRole("heading", { name: "New wall build-up" })).toBeInTheDocument();
  });

  it("lists the build-ups with their layers, thickness and usage, and filters them", async () => {
    const core = await withWall();
    core.buildUps.seed("Slab A", "slab", [
      { material: await concreteId(core), thickness: 0.25, function: "structure" },
    ]);
    const { user } = open(core);
    const wall = await row(/Wall A/);
    expect(wall).toHaveTextContent("2 layers · 213 mm");
    expect(await row(/Slab A/)).toHaveTextContent("1 layer · 250 mm");
    await user.click(screen.getByRole("button", { name: "Slabs" }));
    await waitFor(() => expect(screen.queryByText("Wall A")).not.toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: "Walls" }));
    expect(await row(/Wall A/)).toBeInTheDocument();
  });

  it("says when a filter matches nothing", async () => {
    const { user } = open(await withWall());
    await row(/Wall A/);
    await user.click(screen.getByRole("button", { name: "Slabs" }));
    expect(await screen.findByText("No slab build-ups.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "All" }));
    await user.click(screen.getByRole("button", { name: "Walls" }));
  });

  it("says when the list cannot be read and tries again", async () => {
    const core = await withWall();
    core.failNext("buildups.list", new Error("down"));
    const { user } = open(core);
    expect(await screen.findByText("The build-ups could not be read.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await row(/Wall A/)).toBeInTheDocument();
  });

  it("moves between rows with the arrows and edits with Enter", async () => {
    const core = await withWall();
    core.buildUps.seed("Wall B", "wall", [
      { material: await concreteId(core), thickness: 0.1, function: "structure" },
    ]);
    const { user } = open(core);
    const first = await row(/Wall A/);
    await waitFor(() => expect(first).toHaveAttribute("aria-current", "true"));
    first.focus();
    await user.keyboard("{ArrowDown}");
    const second = await row(/Wall B/);
    await waitFor(() => expect(second).toHaveFocus());
    await user.keyboard("{End}{Home}{ArrowUp}");
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("heading", { name: /Edit build-up/ })).toBeInTheDocument();
  });

  it("edits from the row menu, and from a double click", async () => {
    const { user } = open(await withWall());
    await user.click(await screen.findByRole("button", { name: "Actions for Wall A" }));
    await user.click(screen.getByRole("menuitem", { name: "Edit…" }));
    expect(
      await screen.findByRole("heading", { name: "Edit build-up — Wall A" }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    await user.dblClick(await row(/Wall A/));
    expect(
      await screen.findByRole("heading", { name: "Edit build-up — Wall A" }),
    ).toBeInTheDocument();
  });

  it("closes with Close and with Escape", async () => {
    const { user, onClose } = open(await withWall());
    await row(/Wall A/);
    await user.click(screen.getByRole("button", { name: "Close" }));
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it("is named Build-ups", async () => {
    open(await withWall());
    expect(await screen.findByRole("dialog", { name: "Build-ups" })).toBeInTheDocument();
  });
});

describe("duplicating, renaming and deleting", () => {
  const menu = async (user: ReturnType<typeof open>["user"], item: string) => {
    await user.click(await screen.findByRole("button", { name: "Actions for Wall A" }));
    await user.click(screen.getByRole("menuitem", { name: item }));
  };

  it("duplicates under a suggested name and selects the copy", async () => {
    const { user, core } = open(await withWall());
    await menu(user, "Duplicate…");
    const name = await field("Name of the copy");
    expect(name).toHaveValue("Wall A copy");
    expect(name).toHaveFocus();
    await user.click(screen.getByRole("button", { name: "Duplicate" }));
    expect(await row(/Wall A copy/)).toHaveAttribute("aria-current", "true");
    expect(core.buildUps.names()).toEqual(["Wall A", "Wall A copy"]);
  });

  it("refuses a name that is taken or empty, at the field", async () => {
    const { user } = open(await withWall());
    await menu(user, "Duplicate…");
    const name = await field("Name of the copy");
    await user.clear(name);
    await user.type(name, "wall a{Enter}");
    expect(await screen.findByText(/A build-up with this name already exists/)).toBeInTheDocument();
    await user.clear(name);
    await user.keyboard("{Enter}");
    expect(
      await screen.findByText("Enter a name between 1 and 120 characters."),
    ).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("textbox", { name: "Name of the copy" })).not.toBeInTheDocument();
  });

  it("renames with Enter (F2 starts it), Escape cancels, an unchanged name makes no call", async () => {
    const { user, core } = open(await withWall());
    const wall = await row(/Wall A/);
    wall.focus();
    await user.keyboard("{F2}");
    await user.keyboard("{Escape}");
    expect(await row(/Wall A/)).toBeInTheDocument();
    await user.keyboard("{F2}");
    await user.keyboard("{Enter}");
    expect(calls(core, "buildups.update")).toHaveLength(0);
    await user.keyboard("{F2}");
    const name = await field("Build-up name");
    await user.clear(name);
    await user.type(name, "Facade{Enter}");
    expect(await row(/Facade/)).toBeInTheDocument();
  });

  it("shows a rename refusal at the field", async () => {
    const core = await withWall();
    core.buildUps.seed("Taken", "wall", [
      { material: await concreteId(core), thickness: 0.1, function: "structure" },
    ]);
    const { user } = open(core);
    await menu(user, "Rename…");
    const name = await field("Build-up name");
    await user.clear(name);
    await user.type(name, "Taken{Enter}");
    expect(await screen.findByText(/already exists/)).toBeInTheDocument();
  });

  it("confirms a deletion with the safe button focused, then deletes with Undo", async () => {
    const { user, core } = open(await withWall());
    await menu(user, "Delete…");
    const confirmation = await screen.findByRole("alertdialog", { name: "Delete Wall A?" });
    expect(within(confirmation).getByRole("button", { name: "Keep build-up" })).toHaveFocus();
    expect(
      within(confirmation).getByText("Its 2 layers are deleted with it. You can undo this."),
    ).toBeInTheDocument();
    await user.click(within(confirmation).getByRole("button", { name: "Delete build-up" }));
    await waitFor(() => expect(core.buildUps.names()).toEqual([]));
    expect(await screen.findByText("Wall A deleted")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Undo" })).toBeInTheDocument();
  });

  it("keeps the build-up, with Escape too, and starts deleting with the Delete key", async () => {
    const { user, core } = open(await withWall());
    const wall = await row(/Wall A/);
    await waitFor(() => expect(wall).toHaveAttribute("aria-current", "true"));
    wall.focus();
    await user.keyboard("{Delete}");
    await user.click(await screen.findByRole("button", { name: "Keep build-up" }));
    await user.keyboard("{Delete}");
    await screen.findByRole("alertdialog");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(core.buildUps.names()).toEqual(["Wall A"]);
  });

  it("shows a banner when a deletion fails", async () => {
    const core = await withWall();
    core.override("buildups.delete", () => {
      throw refusal("not_found");
    });
    const { user } = open(core);
    await menu(user, "Delete…");
    await user.click(await screen.findByRole("button", { name: "Delete build-up" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("This build-up no longer exists.");
  });

  it("lists the layers' singular", async () => {
    const core = projectCore();
    core.buildUps.seed("One", "wall", [
      { material: await concreteId(core), thickness: 0.1, function: "structure" },
    ]);
    const { user } = open(core);
    await user.click(await screen.findByRole("button", { name: "Actions for One" }));
    await user.click(screen.getByRole("menuitem", { name: "Delete…" }));
    expect(
      await screen.findByText("Its 1 layer is deleted with it. You can undo this."),
    ).toBeInTheDocument();
  });
});

describe("the editor", () => {
  const editWall = async (core?: Core) => {
    const view = open(await withWall(core));
    await view.user.click(await screen.findByRole("button", { name: "Actions for Wall A" }));
    await view.user.click(screen.getByRole("menuitem", { name: "Edit…" }));
    await screen.findByRole("heading", { name: "Edit build-up — Wall A" });
    return view;
  };

  it("opens an existing build-up on its first layer, with millimetres and defaults", async () => {
    await editWall();
    expect(select("Function of layer 1")).toHaveFocus();
    expect(select("Function of layer 1")).toHaveValue("finish");
    expect(screen.getByRole("textbox", { name: "Thickness of layer 1 (mm)" })).toHaveValue("13");
    expect(select("Join priority of layer 2")).toHaveDisplayValue("1 (default)");
    expect(screen.getByRole("checkbox", { name: "Core, layer 2" })).toBeChecked();
    expect(screen.getAllByText("Exterior").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Interior").length).toBeGreaterThan(0);
    expect(
      screen.getByText("Wall build-up · layers from the exterior to the interior"),
    ).toBeInTheDocument();
  });

  it("shows the totals the core computed", async () => {
    await editWall();
    expect(await screen.findByText("Total thickness: 213 mm")).toBeInTheDocument();
    expect(await screen.findByText("Thermal resistance: 2.13 m²·K/W")).toBeInTheDocument();
  });

  it("follows the draft: a new thickness gives new totals", async () => {
    const { user } = await editWall();
    const thickness = screen.getByRole("textbox", { name: "Thickness of layer 2 (mm)" });
    await user.clear(thickness);
    await user.type(thickness, "100");
    expect(await screen.findByText("Total thickness: 113 mm")).toBeInTheDocument();
  });

  it("says when layers have no thermal data", async () => {
    const core = await withWall();
    const { user } = open(core);
    await user.click(await screen.findByRole("button", { name: "Actions for Wall A" }));
    await user.click(screen.getByRole("menuitem", { name: "Edit…" }));
    await within(select("Material of layer 1")).findByRole("option", { name: "Air" });
    await user.selectOptions(select("Material of layer 1"), "Air");
    expect(
      await screen.findByText(/1 layer without thermal data is not counted/),
    ).toBeInTheDocument();
  });

  it("applies an edit with the layers' ids, and goes back to the list with Undo", async () => {
    const { user, core } = await editWall();
    const thickness = screen.getByRole("textbox", { name: "Thickness of layer 1 (mm)" });
    await user.clear(thickness);
    await user.type(thickness, "15,5");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(await screen.findByText("Build-up applied")).toBeInTheDocument();
    const [update] = calls(core, "buildups.update");
    expect(update?.params).toMatchObject({
      name: "Wall A",
      description: null,
      layers: [{ id: "layer-1", thickness: 0.0155, function: "finish" }, { id: "layer-2" }],
    });
    expect(core.buildUps.layersOf("Wall A")?.[0]?.thickness).toBe(0.0155);
    expect(await row(/Wall A/)).toHaveFocus();
    expect(screen.getByRole("button", { name: "Undo" })).toBeInTheDocument();
  });

  it("closes without a call when nothing changed", async () => {
    const { user, core } = await editWall();
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(await screen.findByRole("heading", { name: "Build-ups" })).toBeInTheDocument();
    expect(calls(core, "buildups.update")).toHaveLength(0);
  });

  it("creates a new build-up: name focused, starter layer, messages first, then saved", async () => {
    const { user, core } = open();
    await user.click(await screen.findByRole("button", { name: "New wall build-up" }));
    expect(await field("Name")).toHaveFocus();
    expect(screen.getAllByRole("row")).toHaveLength(2);
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(
      await screen.findByText("Enter a name between 1 and 120 characters."),
    ).toBeInTheDocument();
    expect(screen.getByText("Choose a material.")).toBeInTheDocument();
    expect(screen.getByText(/Enter a thickness greater than 0 mm/)).toBeInTheDocument();
    expect(screen.getAllByText("3 problems to fix.").length).toBeGreaterThan(0);
    expect(calls(core, "buildups.create")).toHaveLength(0);
    await waitFor(() => expect(field("Name")).resolves.toHaveFocus());

    await user.type(await field("Name"), "Wall B");
    await user.selectOptions(select("Material of layer 1"), "C25/30");
    await user.type(screen.getByRole("textbox", { name: "Thickness of layer 1 (mm)" }), "200");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(await row(/Wall B/)).toHaveAttribute("aria-current", "true");
    expect(calls(core, "buildups.create")[0]?.params).toMatchObject({
      name: "Wall B",
      usage: "wall",
      layers: [{ thickness: 0.2, function: "structure" }],
    });
  });

  it("applies with Enter in the name field", async () => {
    const { user, core } = open();
    await user.click(await screen.findByRole("button", { name: "New slab build-up" }));
    expect(screen.getAllByText("Top").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Bottom").length).toBeGreaterThan(0);
    await user.type(await field("Name"), "Slab X");
    await user.selectOptions(select("Material of layer 1"), "C25/30");
    await user.type(
      screen.getByRole("textbox", { name: "Thickness of layer 1 (mm)" }),
      "250{Enter}",
    );
    await waitFor(() => expect(core.buildUps.names()).toEqual(["Slab X"]));
  });

  it("shows a name refusal under the name, and an unknown layer in a banner", async () => {
    const core = await withWall();
    const { user } = open(core);
    await user.click(await screen.findByRole("button", { name: "New build-up…" }));
    await user.click(screen.getByRole("menuitem", { name: "New wall build-up" }));
    await user.type(await field("Name"), "wall a");
    await user.selectOptions(select("Material of layer 1"), "C25/30");
    await user.type(screen.getByRole("textbox", { name: "Thickness of layer 1 (mm)" }), "100");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(await screen.findByText(/A build-up with this name already exists/)).toBeInTheDocument();

    core.override("buildups.create", () => {
      throw refusal("unknown_layer");
    });
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "This build-up changed since you opened it",
    );
  });

  it("falls back to the generic message for a refusal it does not know", async () => {
    const core = projectCore();
    core.override("buildups.create", () => {
      throw new ApiError(-32000, "x", { reason: "something_new" });
    });
    const { user } = open(core);
    await user.click(await screen.findByRole("button", { name: "New wall build-up" }));
    await user.type(await field("Name"), "X");
    await user.selectOptions(select("Material of layer 1"), "C25/30");
    await user.type(screen.getByRole("textbox", { name: "Thickness of layer 1 (mm)" }), "100");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(await screen.findByText("Something went wrong. Try again.")).toBeInTheDocument();
  });
});

describe("layers", () => {
  const edit = async () => {
    const view = open(await withWall());
    await view.user.click(await screen.findByRole("button", { name: "Actions for Wall A" }));
    await view.user.click(screen.getByRole("menuitem", { name: "Edit…" }));
    await screen.findByRole("heading", { name: "Edit build-up — Wall A" });
    return view;
  };
  const live = () =>
    screen
      .getAllByRole("status")
      .map((node) => node.textContent)
      .join("|");

  it("moves a layer with its button, keeps focus on it and announces the new place", async () => {
    const { user } = await edit();
    await user.click(screen.getByRole("button", { name: "Move layer 2 up" }));
    expect(select("Function of layer 1")).toHaveValue("structure");
    expect(screen.getByRole("button", { name: "Move layer 1 up" })).toHaveFocus();
    expect(live()).toContain("Layer moved to position 1 of 2: C25/30, 200 mm.");
  });

  it("moves with Alt+Arrow from a button and from the core checkbox, not from a field", async () => {
    const { user } = await edit();
    screen.getByRole("button", { name: "Move layer 1 down" }).focus();
    await user.keyboard("{Alt>}{ArrowDown}{/Alt}");
    expect(select("Function of layer 2")).toHaveValue("finish");
    expect(screen.getByRole("button", { name: "Move layer 2 down" })).toHaveFocus();
    await user.keyboard("{Alt>}{ArrowUp}{/Alt}");
    expect(select("Function of layer 1")).toHaveValue("finish");
    screen.getByRole("checkbox", { name: "Core, layer 1" }).focus();
    await user.keyboard("{Alt>}{ArrowDown}{/Alt}");
    expect(select("Function of layer 2")).toHaveValue("finish");
    screen.getByRole("textbox", { name: "Thickness of layer 2 (mm)" }).focus();
    await user.keyboard("{Alt>}{ArrowUp}{/Alt}");
    expect(select("Function of layer 2")).toHaveValue("finish");
  });

  it("says when a layer cannot go further", async () => {
    const { user } = await edit();
    await user.click(screen.getByRole("button", { name: "Move layer 1 up" }));
    expect(live()).toContain("Already the first layer.");
    await user.click(screen.getByRole("button", { name: "Move layer 2 down" }));
    expect(live()).toContain("Already the last layer.");
    expect(screen.getByRole("button", { name: "Move layer 1 up" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
  });

  it("adds a layer after the active one, focusing its function, with Ctrl+Enter or the button", async () => {
    const { user } = await edit();
    await user.click(screen.getByRole("textbox", { name: "Thickness of layer 1 (mm)" }));
    await user.keyboard("{Control>}{Enter}{/Control}");
    expect(select("Function of layer 2")).toHaveFocus();
    expect(screen.getAllByRole("row")).toHaveLength(4);
    await user.click(screen.getByRole("button", { name: "Add layer" }));
    expect(screen.getAllByRole("row")).toHaveLength(5);
    expect(live()).toContain("Layer added at position");
  });

  it("stops at 32 layers", async () => {
    const { user } = await edit();
    for (let count = 2; count < 32; count++) {
      await user.click(screen.getByRole("button", { name: "Add layer" }));
    }
    const add = screen.getByRole("button", { name: "Add layer" });
    expect(add).toHaveAttribute("aria-disabled", "true");
    expect(add).toHaveAttribute("title", "A build-up can have at most 32 layers.");
    await user.click(add);
    expect(screen.getAllByRole("combobox", { name: /^Function of layer/ })).toHaveLength(32);
  }, 30000);

  it("removes a layer, keeps focus on the same control and announces it; the last one stays", async () => {
    const { user } = await edit();
    await user.click(screen.getByRole("button", { name: "Remove layer 1" }));
    expect(screen.getByRole("button", { name: "Remove layer 1" })).toHaveFocus();
    expect(live()).toContain("Layer removed. 1 layer left.");
    expect(screen.getByRole("button", { name: "Remove layer 1" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    await user.click(screen.getByRole("button", { name: "Remove layer 1" }));
    expect(screen.getAllByRole("combobox", { name: /^Function of layer/ })).toHaveLength(1);
  });

  it("removes the last row and focuses the previous one", async () => {
    const { user } = await edit();
    await user.click(screen.getByRole("button", { name: "Remove layer 2" }));
    expect(screen.getByRole("button", { name: "Remove layer 1" })).toHaveFocus();
  });

  it("offers an air gap generic materials only", async () => {
    const { user } = await edit();
    expect(
      within(select("Material of layer 1")).getByRole("option", { name: "C25/30" }),
    ).toBeInTheDocument();
    await user.selectOptions(select("Function of layer 1"), "air_gap");
    expect(
      within(select("Material of layer 1")).queryByRole("option", { name: "C25/30" }),
    ).not.toBeInTheDocument();
    expect(
      within(select("Material of layer 1")).getByRole("option", { name: "Air" }),
    ).toBeInTheDocument();
  });

  it("keeps defaults while the function changes, and marks a chosen priority as modified", async () => {
    const { user } = await edit();
    expect(screen.getByRole("checkbox", { name: "Core, layer 1" })).not.toBeChecked();
    await user.selectOptions(select("Function of layer 1"), "structure");
    expect(screen.getByRole("checkbox", { name: "Core, layer 1" })).toBeChecked();
    expect(select("Join priority of layer 1")).toHaveDisplayValue("1 (default)");
    await user.selectOptions(select("Join priority of layer 1"), "4");
    expect(screen.getByText("Modified")).toBeInTheDocument();
    await user.selectOptions(select("Function of layer 1"), "finish");
    expect(select("Join priority of layer 1")).toHaveValue("4");
    await user.selectOptions(select("Join priority of layer 1"), "default");
    expect(screen.queryByText("Modified")).not.toBeInTheDocument();
    expect(select("Join priority of layer 1")).toHaveDisplayValue("5 (default)");
  });

  it("explains the join priority and the core, and closes the help with Escape", async () => {
    const { user } = await edit();
    const help = screen.getByRole("button", { name: "More about About join priority" });
    expect(help).toHaveAttribute("aria-expanded", "false");
    await user.click(help);
    expect(screen.getByText(/layers with a higher priority \(1\) run through/)).toBeVisible();
    await user.keyboard("{Escape}");
    expect(help).toHaveAttribute("aria-expanded", "false");
    await user.click(screen.getByRole("button", { name: "More about Core" }));
    expect(screen.getByText(/The core is the load-bearing part/)).toBeVisible();
  });

  it("flags core layers that are not next to each other, on the rows concerned", async () => {
    const { user } = await edit();
    await user.click(screen.getByRole("button", { name: "Add layer" }));
    // The new layer goes after the active one: it is layer 2.
    await within(select("Material of layer 2")).findByRole("option", { name: "C25/30" });
    await user.selectOptions(select("Material of layer 2"), "C25/30");
    await user.type(screen.getByRole("textbox", { name: "Thickness of layer 2 (mm)" }), "20");
    await user.click(screen.getByRole("checkbox", { name: "Core, layer 1" }));
    await user.click(screen.getByRole("checkbox", { name: "Core, layer 2" }));
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(await screen.findByText("Core layers must be next to each other.")).toBeInTheDocument();
    expect(screen.getByText("This layer is between core layers.")).toBeInTheDocument();
  });

  it("explains a thickness of 0 and an air gap with the wrong material from the core", async () => {
    const { user } = await edit();
    const thickness = screen.getByRole("textbox", { name: "Thickness of layer 1 (mm)" });
    await user.clear(thickness);
    await user.type(thickness, "0");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(await screen.findByText(/Enter a thickness greater than 0 mm/)).toBeInTheDocument();
    expect(thickness).toHaveAccessibleDescription(/greater than 0 mm/);
  });

  it("shows a priority error and a vanished material from the core", async () => {
    const core = await withWall();
    const { user } = open(core);
    await user.click(await screen.findByRole("button", { name: "Actions for Wall A" }));
    await user.click(screen.getByRole("menuitem", { name: "Edit…" }));
    await user.click(screen.getByRole("button", { name: "Move layer 2 up" }));
    core.override("buildups.preview", () => ({
      errors: [
        { code: "invalid_priority", index: 0 },
        { code: "unknown_material", index: 1 },
        { code: "invalid_material", index: 5 },
        { code: "too_many_layers" },
        { code: "no_layers" },
      ],
      layers: [],
      total_thickness: 0,
      thermal_resistance: null,
      layers_without_thermal_data: 0,
    }));
    await user.type(screen.getByRole("textbox", { name: "Thickness of layer 1 (mm)" }), "1");
    // The overridden answer arrives once the draft has stopped changing.
    expect(await screen.findByText("Total thickness: 0 mm")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(await screen.findByText("Choose a priority from 1 to 5.")).toBeInTheDocument();
    expect(screen.getByText(/no longer exists in the project/)).toBeInTheDocument();
    expect(screen.getByText("Add at least one layer.")).toBeInTheDocument();
  });

  it("shows em dashes when the core cannot answer", async () => {
    const core = await withWall();
    core.override("buildups.preview", () => {
      throw new Error("down");
    });
    const { user } = open(core);
    await user.click(await screen.findByRole("button", { name: "Actions for Wall A" }));
    await user.click(screen.getByRole("menuitem", { name: "Edit…" }));
    expect(await screen.findByText("Thermal resistance: —")).toBeInTheDocument();
    expect(screen.getByText("Total thickness: — mm")).toBeInTheDocument();
  });
});

describe("leaving the editor", () => {
  const edit = async () => {
    const view = open(await withWall());
    await view.user.click(await screen.findByRole("button", { name: "Actions for Wall A" }));
    await view.user.click(screen.getByRole("menuitem", { name: "Edit…" }));
    await screen.findByRole("heading", { name: "Edit build-up — Wall A" });
    return view;
  };

  it("cancels at once when nothing changed, with the button and with Escape", async () => {
    const { user } = await edit();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(await screen.findByRole("heading", { name: "Build-ups" })).toBeInTheDocument();
    await user.dblClick(await row(/Wall A/));
    await screen.findByRole("heading", { name: /Edit build-up/ });
    await user.keyboard("{Escape}");
    expect(await screen.findByRole("heading", { name: "Build-ups" })).toBeInTheDocument();
  });

  it("asks before discarding changes, with the safe button focused", async () => {
    const { user, core } = await edit();
    await user.type(await field("Name"), "!");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    const confirmation = await screen.findByRole("alertdialog", { name: "Discard your changes?" });
    expect(within(confirmation).getByRole("button", { name: "Keep editing" })).toHaveFocus();
    await user.click(within(confirmation).getByRole("button", { name: "Keep editing" }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    await user.keyboard("{Escape}");
    await screen.findByRole("alertdialog");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    await user.keyboard("{Escape}");
    await user.click(await screen.findByRole("button", { name: "Discard changes" }));
    expect(await screen.findByRole("heading", { name: "Build-ups" })).toBeInTheDocument();
    expect(calls(core, "buildups.update")).toHaveLength(0);
  });
});

describe("the materials of a layer", () => {
  it("does not offer the Materials dialog as an option of the select", async () => {
    const { user } = open();
    await user.click(await screen.findByRole("button", { name: "New wall build-up" }));
    expect(
      within(select("Material of layer 1")).queryByRole("option", { name: "Manage materials…" }),
    ).not.toBeInTheDocument();
  });

  it("opens the Materials dialog over the editor and picks the material that was added", async () => {
    const { user } = open();
    await user.click(await screen.findByRole("button", { name: "New wall build-up" }));
    await user.click(
      screen.getAllByRole("button", { name: "Manage materials…" })[0] as HTMLElement,
    );
    const materials = await screen.findByRole("dialog", { name: "Materials" });
    await user.click(await within(materials).findByRole("button", { name: "New material…" }));
    await user.type(within(materials).getByRole("textbox", { name: "Name" }), "Brick");
    await user.click(within(materials).getByRole("button", { name: "Create" }));
    await within(materials).findByRole("heading", { name: "Brick" });
    await user.click(within(materials).getByRole("button", { name: "Done" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Materials" })).not.toBeInTheDocument(),
    );
    await waitFor(() => expect(select("Material of layer 1")).toHaveDisplayValue("Brick"));
  });

  it("keeps the layer's material when nothing was added", async () => {
    const { user } = open();
    await user.click(await screen.findByRole("button", { name: "New wall build-up" }));
    await user.selectOptions(select("Material of layer 1"), "C25/30");
    await user.click(
      screen.getAllByRole("button", { name: "Manage materials…" })[0] as HTMLElement,
    );
    await user.click(await screen.findByRole("button", { name: "Done" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Materials" })).not.toBeInTheDocument(),
    );
    expect(select("Material of layer 1")).toHaveDisplayValue("C25/30");
  });
});

describe("a change behind the dialog's back", () => {
  it("refreshes the list", async () => {
    const core = await withWall();
    open(core);
    await row(/Wall A/);
    core.buildUps.seed("Wall Z", "wall", [
      { material: await concreteId(core), thickness: 0.1, function: "structure" },
    ]);
    await act(async () => core.notifyChange());
    expect(await row(/Wall Z/)).toBeInTheDocument();
  });

  it("reads a build-up that cannot be opened as a banner", async () => {
    const core = await withWall();
    core.override("buildups.get", () => {
      throw refusal("not_found");
    });
    const { user } = open(core);
    await user.dblClick(await row(/Wall A/));
    expect(await screen.findByRole("alert")).toHaveTextContent("This build-up no longer exists.");
  });
});
