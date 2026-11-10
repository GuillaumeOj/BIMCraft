import { act, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../api";
import { OVERRIDE_STORAGE_KEY } from "../i18n/I18nProvider";
import { fakeCore, refusal } from "../test-support/fakeCore";
import { renderWithCore } from "../test-support/render";
import { MaterialsDialog } from "./MaterialsDialog";

const C25 = "en1992-1-1/C25/30";
const C30 = "en1992-1-1/C30/37";
const S355 = "en1993-1-1/S355";

type Core = ReturnType<typeof fakeCore>;

function open(core: Core = fakeCore(), onClose = vi.fn()) {
  const view = renderWithCore(<MaterialsDialog onClose={onClose} />, core);
  return { ...view, core, onClose };
}

/** A core with the grades `keys` already in the project. */
function seeded(...keys: string[]) {
  const core = fakeCore();
  for (const key of keys) core.materials.seed(key);
  return core;
}

const inList = async (list: string, name: string | RegExp) =>
  within(await screen.findByRole("listbox", { name: list })).findByRole("option", { name });
/** A row of the project list. */
const option = (name: string | RegExp) => inList("Project materials", name);
/** A grade of the library. */
const grade = (name: string | RegExp) => inList("Library grades", name);
const editButton = (property: string) =>
  screen.findByRole("button", { name: new RegExp(`^Edit ${property}`) });
const calls = (core: Core, method: string) => core.calls.filter((call) => call.method === method);

afterEach(() => {
  localStorage.clear();
});

describe("the empty project", () => {
  it("explains and opens the library with nothing to go back to", async () => {
    open();
    expect(await screen.findByText(/No materials yet/)).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Add from library" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^←/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("searchbox")).not.toBeInTheDocument();
  });

  it("adds a grade: it is selected, shown with its source, and announced with an Undo", async () => {
    const { user, core } = open();
    await user.click(await grade(/^C25\/30/));
    expect(screen.getByRole("textbox", { name: "Name in this project" })).toHaveValue("C25/30");
    await user.click(screen.getByRole("button", { name: "Add to project" }));

    expect(await screen.findByRole("heading", { name: "C25/30" })).toBeInTheDocument();
    expect(await editButton("Characteristic compressive strength")).toHaveTextContent("25 MPa");
    expect(screen.getAllByText("EN 1992-1-1:2004 · Table 3.1").length).toBeGreaterThan(0);
    expect(screen.getByText("Concrete · EN 1992-1-1")).toBeInTheDocument();
    expect(screen.getByText("Behaviour law: parabola-rectangle")).toBeInTheDocument();
    expect(screen.getByRole("listbox", { name: "Project materials" })).toHaveFocus();
    expect(core.materials.names()).toEqual(["C25/30"]);

    const toast = await screen.findByText("C25/30 added");
    expect(
      within(toast.parentElement as HTMLElement).getByRole("button", { name: "Undo" }),
    ).toBeInTheDocument();
  });

  it("adds under another name, and keeps the name field in error on a duplicate", async () => {
    const { user, core } = open(seeded(C25));
    await user.click(await screen.findByRole("button", { name: "Add from library…" }));
    await user.click(await grade(/^C25\/30/));
    await user.click(screen.getByRole("button", { name: "Add to project" }));
    expect(await screen.findByText(/A material with this name already exists/)).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Name in this project" })).toHaveFocus();

    const name = screen.getByRole("textbox", { name: "Name in this project" });
    await user.clear(name);
    await user.type(name, "Slab concrete{Enter}");
    expect(await screen.findByRole("heading", { name: "Slab concrete" })).toBeInTheDocument();
    expect(calls(core, "materials.add_from_library").at(-1)?.params).toMatchObject({
      key: C25,
      name: "Slab concrete",
    });
  });

  it("refuses an empty name inline", async () => {
    const { user } = open(seeded(C25));
    await user.click(await screen.findByRole("button", { name: "Add from library…" }));
    await user.click(await grade(/^C30\/37/));
    await user.clear(screen.getByRole("textbox", { name: "Name in this project" }));
    await user.click(screen.getByRole("button", { name: "Add to project" }));
    expect(
      await screen.findByText("Enter a name between 1 and 120 characters."),
    ).toBeInTheDocument();
  });
});

describe("the project list", () => {
  it("selects the first row and shows its category, grade and use", async () => {
    const core = seeded(C25, S355);
    core.materials.setUsedBy("C25/30", 3);
    open(core);
    const first = await option(/^C25\/30/);
    await waitFor(() => expect(first).toHaveAttribute("aria-selected", "true"));
    expect(first).toHaveTextContent("Concrete · C25/30");
    expect(first).toHaveTextContent("Used by 3");
    expect(await option(/^S355/)).toHaveAttribute("aria-selected", "false");
    expect(await screen.findByRole("searchbox", { name: "Search materials" })).toHaveFocus();
  });

  it("searches by name, and says when nothing matches", async () => {
    const { user } = open(seeded(C25, S355));
    await user.type(await screen.findByRole("searchbox"), "s35");
    await waitFor(() =>
      expect(screen.queryByRole("option", { name: /C25/ })).not.toBeInTheDocument(),
    );
    expect(await option(/^S355/)).toBeInTheDocument();
    await user.clear(screen.getByRole("searchbox"));
    await user.type(screen.getByRole("searchbox"), "zzz");
    expect(await screen.findByText("No material matches “zzz”.")).toBeInTheDocument();
  });

  it("filters by category", async () => {
    const { user } = open(seeded(C25, S355));
    await user.selectOptions(await screen.findByRole("combobox", { name: "Category" }), "steel");
    await waitFor(() =>
      expect(screen.queryByRole("option", { name: /C25/ })).not.toBeInTheDocument(),
    );
    await user.selectOptions(screen.getByRole("combobox", { name: "Category" }), "");
    expect(await option(/^C25/)).toBeInTheDocument();
  });

  it("moves the selection with the arrows, Home and End", async () => {
    const { user } = open(seeded(C25, C30, S355));
    await option(/^C25/);
    await user.tab();
    await user.tab();
    expect(screen.getByRole("listbox", { name: "Project materials" })).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(await option(/^C30/)).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{End}");
    expect(await option(/^S355/)).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{ArrowUp}{Home}");
    expect(await option(/^C25/)).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByRole("heading", { name: "C25/30" })).toBeInTheDocument();
    await user.click(await option(/^S355/));
    expect(await screen.findByRole("heading", { name: "S355" })).toBeInTheDocument();
  });

  it("says so when the list cannot be read", async () => {
    const core = seeded(C25);
    core.failNext("materials.list", new Error("down"));
    open(core);
    expect(await screen.findByText("The materials could not be read.")).toBeInTheDocument();
  });

  it("follows a change made elsewhere", async () => {
    const core = seeded(C25);
    const { user } = open(core);
    await option(/^C25/);
    core.materials.seed(S355);
    await act(async () => core.notifyChange());
    expect(await option(/^S355/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Done" }));
  });

  it("shows a banner when the selected material cannot be read", async () => {
    const core = seeded(C25);
    core.override("materials.get", () => {
      throw refusal("not_found");
    });
    open(core);
    expect(await screen.findByText("This material no longer exists.")).toBeInTheDocument();
  });
});

describe("the detail", () => {
  it("shows the Mechanical properties, with the partial factor on two lines", async () => {
    open(seeded(C25));
    expect(await editButton("Mean compressive strength")).toHaveTextContent("33 MPa");
    expect(await editButton("Modulus of elasticity")).toHaveTextContent("31 GPa");
    expect(
      screen.getByRole("button", { name: /Partial factor for concrete: Recommended 1.5/ }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Partial factor for concrete: France 1.5/ }),
    ).toBeInTheDocument();
  });

  it("opens the Thermal tab, and keeps Graphics and Environmental unavailable", async () => {
    const { user } = open(seeded(C25));
    await user.click(await screen.findByRole("tab", { name: "Thermal" }));
    expect(await editButton("Thermal conductivity")).toHaveTextContent("2.3 W/(m·K)");
    expect(
      screen.queryByText("Characteristic compressive strength", { exact: false }),
    ).not.toBeInTheDocument();

    const graphics = screen.getByRole("tab", { name: "Graphics" });
    expect(graphics).toHaveAttribute("aria-disabled", "true");
    expect(graphics).toHaveAttribute("title", "Not available yet.");
    await user.click(graphics);
    expect(screen.getByRole("tab", { name: "Thermal" })).toHaveAttribute("aria-selected", "true");
  });

  it("moves between the tabs with the arrows, Home and End, skipping disabled ones for opening", async () => {
    const { user } = open(seeded(C25));
    const mechanical = await screen.findByRole("tab", { name: "Mechanical" });
    mechanical.focus();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Thermal" })).toHaveFocus();
    expect(screen.getByRole("tab", { name: "Thermal" })).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Graphics" })).toHaveFocus();
    expect(screen.getByRole("tab", { name: "Thermal" })).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{End}");
    expect(screen.getByRole("tab", { name: "Environmental" })).toHaveFocus();
    await user.keyboard("{ArrowRight}");
    expect(mechanical).toHaveFocus();
    await user.keyboard("{ArrowLeft}");
    expect(screen.getByRole("tab", { name: "Environmental" })).toHaveFocus();
    await user.keyboard("{Home}");
    expect(mechanical).toHaveFocus();
    await user.keyboard("a");
  });

  it("goes back to Mechanical when another material is selected", async () => {
    const { user } = open(seeded(C25, S355));
    await user.click(await screen.findByRole("tab", { name: "Thermal" }));
    await user.click(await option(/^S355/));
    expect(await screen.findByRole("tab", { name: "Mechanical" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });
});

describe("editing a value", () => {
  it("saves with Enter, marks the value as entered by the user and resets it", async () => {
    const { user, core } = open(seeded(C25));
    await user.click(await editButton("Characteristic compressive strength"));
    const field = screen.getByRole("textbox", {
      name: "Characteristic compressive strength (MPa)",
    });
    expect(field).toHaveFocus();
    expect(field).toHaveValue("25");
    await user.clear(field);
    await user.type(field, "30{Enter}");

    await waitFor(() => expect(screen.getAllByText("Entered by user").length).toBeGreaterThan(0));
    expect(await editButton("Characteristic compressive strength")).toHaveTextContent("30 MPa");
    expect(calls(core, "materials.update").at(-1)?.params).toMatchObject({
      properties: [{ key: "fck", value: 30e6 }],
    });

    await user.hover(screen.getByRole("button", { name: /Reset to library value/ }));
    expect(screen.getByRole("tooltip")).toHaveTextContent("Library value: 25 MPa");
    await user.click(screen.getByRole("button", { name: /Reset to library value/ }));
    await waitFor(() => expect(screen.queryByText("Entered by user")).not.toBeInTheDocument());
    expect(await editButton("Characteristic compressive strength")).toHaveTextContent("25 MPa");
    expect(
      screen.getByText("Characteristic compressive strength reset to the library value."),
    ).toBeInTheDocument();
  });

  it("saves when focus leaves the field, and does nothing when the value is unchanged", async () => {
    const { user, core } = open(seeded(C25));
    await user.click(await editButton("Mean compressive strength"));
    await user.tab();
    expect(calls(core, "materials.update")).toHaveLength(0);
    await user.click(await editButton("Mean compressive strength"));
    const field = screen.getByRole("textbox", { name: "Mean compressive strength (MPa)" });
    await user.clear(field);
    await user.type(field, "35");
    await user.tab();
    expect(
      await screen.findByRole("button", { name: /Mean compressive strength: 35\sMPa/ }),
    ).toBeInTheDocument();
  });

  it("cancels with Escape without closing the dialog", async () => {
    const { user, core, onClose } = open(seeded(C25));
    await user.click(await editButton("Characteristic compressive strength"));
    await user.keyboard("9{Escape}");
    expect(await editButton("Characteristic compressive strength")).toHaveTextContent("25 MPa");
    expect(calls(core, "materials.update")).toHaveLength(0);
    expect(onClose).not.toHaveBeenCalled();
  });

  it("opens the editor with F2", async () => {
    const { user } = open(seeded(C25));
    (await editButton("Characteristic compressive strength")).focus();
    await user.keyboard("{F2}");
    expect(
      screen.getByRole("textbox", { name: "Characteristic compressive strength (MPa)" }),
    ).toBeInTheDocument();
  });

  it("keeps the field open with the allowed range when the core refuses the value", async () => {
    const { user } = open(seeded(C25));
    await user.click(await screen.findByRole("button", { name: /Add value Poisson/ }));
    const field = await screen.findByRole("textbox", { name: /Poisson's ratio/ });
    await user.type(field, "0.6{Enter}");
    const message = await screen.findByText(
      "Enter a valid value for this property. Between 0 and 0.5.",
    );
    expect(field).toHaveAttribute("aria-invalid", "true");
    expect(field).toHaveAttribute("aria-describedby", message.id);
    expect(field).toHaveFocus();
  });

  it("says that a value must be above an excluded bound, not at least that bound", async () => {
    const { user } = open(seeded(C25));
    await user.click(await editButton("Characteristic compressive strength"));
    const field = screen.getByRole("textbox", { name: /strength \(MPa\)/ });
    await user.clear(field);
    await user.type(field, "0{Enter}");
    expect(
      await screen.findByText("Enter a valid value for this property. More than 0."),
    ).toBeInTheDocument();
  });

  it("refuses text that is not a number without asking the core", async () => {
    const { user, core } = open(seeded(C25));
    await user.click(await editButton("Characteristic compressive strength"));
    await user.type(screen.getByRole("textbox", { name: /strength \(MPa\)/ }), "x{Enter}");
    expect(await screen.findByText(/Enter a valid value for this property/)).toBeInTheDocument();
    expect(calls(core, "materials.update")).toHaveLength(0);
  });

  it("says that a required value cannot be cleared", async () => {
    const { user } = open();
    await user.click(await screen.findByRole("button", { name: "New material…" }));
    await user.type(screen.getByRole("textbox", { name: "Name" }), "Mix");
    await user.click(screen.getByRole("button", { name: "Create" }));
    await user.click(
      await screen.findByRole("button", { name: /Add value Characteristic compressive strength/ }),
    );
    await user.type(screen.getByRole("textbox", { name: /strength \(MPa\)/ }), "20{Enter}");
    await user.click(await editButton("Characteristic compressive strength"));
    await user.clear(screen.getByRole("textbox", { name: /strength \(MPa\)/ }));
    await user.keyboard("{Enter}");
    expect(await screen.findByText("This value is required.")).toBeInTheDocument();
  });

  it("shows a banner, not a field error, for any other failure", async () => {
    const core = seeded(C25);
    const { user } = open(core);
    core.failNext("materials.update", new Error("down"));
    await user.click(await editButton("Mean compressive strength"));
    const field = screen.getByRole("textbox", { name: /strength \(MPa\)/ });
    await user.clear(field);
    await user.type(field, "36{Enter}");
    expect(await screen.findByRole("alert")).toHaveTextContent("Something went wrong. Try again.");
    expect(screen.queryByRole("textbox", { name: /strength \(MPa\)/ })).not.toBeInTheDocument();
  });

  it("shows a banner when a reset fails", async () => {
    const core = seeded(C25);
    const { user } = open(core);
    await user.click(await editButton("Mean compressive strength"));
    const field = screen.getByRole("textbox", { name: /strength \(MPa\)/ });
    await user.clear(field);
    await user.type(field, "36{Enter}");
    const reset = await screen.findByRole("button", { name: /Reset to library value/ });
    core.failNext("materials.update", new Error("down"));
    await user.click(reset);
    expect(await screen.findByRole("alert")).toBeInTheDocument();
  });

  it("adds a value that the material did not have, and clears it again", async () => {
    const { user, core } = open(seeded(S355));
    await user.click(await screen.findByRole("button", { name: /Add value Density/ }));
    await user.type(screen.getByRole("textbox", { name: "Density (kg/m³)" }), "7850{Enter}");
    expect(await editButton("Density")).toHaveTextContent("7850 kg/m³");
    await user.click(await editButton("Density"));
    await user.clear(screen.getByRole("textbox", { name: "Density (kg/m³)" }));
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("button", { name: /Add value Density/ })).toBeInTheDocument();
    expect(calls(core, "materials.update").at(-1)?.params).toMatchObject({
      properties: [{ key: "density", value: null }],
    });
  });

  it("accepts a decimal comma in French, and shows one", async () => {
    localStorage.setItem(OVERRIDE_STORAGE_KEY, "fr");
    const { user, core } = open(seeded(C25));
    await user.click(
      await screen.findByRole("button", { name: /^Modifier Résistance moyenne en compression/ }),
    );
    const field = screen.getByRole("textbox", {
      name: /Résistance moyenne en compression \(MPa\)/,
    });
    await user.clear(field);
    await user.type(field, "33,5{Enter}");
    expect(await screen.findByRole("button", { name: /: 33,5\sMPa/ })).toBeInTheDocument();
    expect(calls(core, "materials.update").at(-1)?.params).toMatchObject({
      properties: [{ key: "fcm", value: 33.5e6 }],
    });
    expect(screen.getByText("Saisie par l’utilisateur")).toBeInTheDocument();
  });

  it("edits one annex of a partial factor", async () => {
    const { user, core } = open(seeded(C25));
    await user.click(
      await screen.findByRole("button", { name: /Partial factor for concrete: France/ }),
    );
    await user.type(
      screen.getByRole("textbox", { name: /France Partial factor/ }),
      "{Backspace}{Backspace}{Backspace}1.4{Enter}",
    );
    await waitFor(() =>
      expect(calls(core, "materials.update").at(-1)?.params).toMatchObject({
        properties: [{ key: "gamma_c", annex: "fr", value: 1.4 }],
      }),
    );
    expect(
      await screen.findByRole("button", { name: /Partial factor for concrete: France 1.4/ }),
    ).toBeInTheDocument();
  });
});

describe("a new material", () => {
  it("is created with a name and a category, then opened with every value empty", async () => {
    const { user, core } = open(seeded(C25));
    await user.click(await screen.findByRole("button", { name: "New material…" }));
    expect(screen.getByRole("textbox", { name: "Name" })).toHaveFocus();
    await user.type(screen.getByRole("textbox", { name: "Name" }), "Mix A");
    const form = screen.getByRole("form", { name: "New material" });
    await user.selectOptions(within(form).getByRole("combobox", { name: "Category" }), "steel");
    await user.click(screen.getByRole("button", { name: "Create" }));
    expect(await screen.findByRole("heading", { name: "Mix A" })).toBeInTheDocument();
    expect(screen.getByText("Steel · No design code")).toBeInTheDocument();
    expect(await screen.findAllByRole("button", { name: /Add value/ })).toHaveLength(4);
    expect(calls(core, "materials.create")[0]?.params).toMatchObject({
      name: "Mix A",
      category: "steel",
    });
    expect(screen.getByText("Behaviour law: linear elastic")).toBeInTheDocument();
    expect(screen.getByText("Used for section design. Not editable.")).toBeInTheDocument();
  });

  it("asks for a name and refuses a duplicate", async () => {
    const { user } = open(seeded(C25));
    await user.click(await screen.findByRole("button", { name: "New material…" }));
    await user.click(screen.getByRole("button", { name: "Create" }));
    expect(
      await screen.findByText("Enter a name between 1 and 120 characters."),
    ).toBeInTheDocument();
    await user.type(screen.getByRole("textbox", { name: "Name" }), "c25/30");
    expect(
      screen.queryByText("Enter a name between 1 and 120 characters."),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Create" }));
    expect(await screen.findByText(/already exists/)).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Name" })).toHaveFocus();
  });

  it("shows a banner for another failure, and goes back to the material", async () => {
    const core = seeded(C25);
    const { user } = open(core);
    await user.click(await screen.findByRole("button", { name: "New material…" }));
    await user.type(screen.getByRole("textbox", { name: "Name" }), "Mix");
    core.failNext("materials.create", new Error("down"));
    await user.click(screen.getByRole("button", { name: "Create" }));
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "← Back to C25/30" }));
    expect(await screen.findByRole("heading", { name: "C25/30" })).toBeInTheDocument();
  });
});

describe("renaming", () => {
  const rename = async (user: ReturnType<typeof open>["user"]) => {
    await user.click(await screen.findByRole("button", { name: "Actions for C25/30" }));
    await user.click(screen.getByRole("menuitem", { name: "Rename…" }));
    return screen.getByRole("textbox", { name: "Material name" });
  };

  it("renames from the menu with Enter", async () => {
    const { user } = open(seeded(C25));
    const field = await rename(user);
    await user.clear(field);
    await user.type(field, "Slab{Enter}");
    expect(await screen.findByRole("heading", { name: "Slab" })).toBeInTheDocument();
    expect(await option(/^Slab/)).toBeInTheDocument();
  });

  it("starts with F2 in the list, and Escape leaves the name unchanged", async () => {
    const { user, onClose } = open(seeded(C25));
    await option(/^C25/);
    screen.getByRole("listbox", { name: "Project materials" }).focus();
    await user.keyboard("{F2}");
    await user.keyboard("{Escape}");
    expect(await screen.findByRole("heading", { name: "C25/30" })).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("leaves without a call when the name is the same", async () => {
    const { user, core } = open(seeded(C25));
    await rename(user);
    await user.keyboard("{Enter}");
    expect(calls(core, "materials.update")).toHaveLength(0);
  });

  it.each([
    ["", "Enter a name between 1 and 120 characters."],
    ["S355", "A material with this name already exists. Choose another name."],
  ])("refuses the name %j", async (name, message) => {
    const { user } = open(seeded(C25, S355));
    const field = await rename(user);
    await user.clear(field);
    if (name) await user.type(field, name);
    await user.keyboard("{Enter}");
    expect(await screen.findByText(message)).toBeInTheDocument();
  });

  it("shows a banner for another failure", async () => {
    const core = seeded(C25);
    const { user } = open(core);
    const field = await rename(user);
    core.failNext("materials.update", new Error("down"));
    await user.type(field, "x{Enter}");
    expect(await screen.findByRole("alert")).toBeInTheDocument();
  });

  it("closes the menu with Escape", async () => {
    const { user } = open(seeded(C25));
    const button = await screen.findByRole("button", { name: "Actions for C25/30" });
    await user.click(button);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    await user.click(button);
    button.focus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });
});

describe("deleting", () => {
  const ask = async (user: ReturnType<typeof open>["user"], name = "C25/30") => {
    await user.click(await screen.findByRole("button", { name: `Actions for ${name}` }));
    await user.click(screen.getByRole("menuitem", { name: "Delete…" }));
  };

  it("confirms inline with the safe button focused, then deletes and offers Undo", async () => {
    const { user, core } = open(seeded(C25, S355));
    await ask(user);
    const confirmation = screen.getByRole("alertdialog", { name: "Delete C25/30?" });
    expect(within(confirmation).getByRole("button", { name: "Keep material" })).toHaveFocus();
    expect(
      screen.getByText("This removes the material from the project. You can undo this."),
    ).toBeInTheDocument();
    await user.click(within(confirmation).getByRole("button", { name: "Delete material" }));

    expect(await screen.findByRole("heading", { name: "S355" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /C25/ })).not.toBeInTheDocument();
    expect(core.materials.names()).toEqual(["S355"]);
    await user.click(await screen.findByRole("button", { name: "Undo" }));
    expect(await option(/^C25\/30/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Undo" })).not.toBeInTheDocument();
  });

  it("selects the previous material after deleting the last one, and the library after the only one", async () => {
    const { user } = open(seeded(C25, S355));
    await user.click(await option(/^S355/));
    await ask(user, "S355");
    await user.click(screen.getByRole("button", { name: "Delete material" }));
    expect(await screen.findByRole("heading", { name: "C25/30" })).toBeInTheDocument();
    await ask(user);
    await user.click(screen.getByRole("button", { name: "Delete material" }));
    expect(await screen.findByText(/No materials yet/)).toBeInTheDocument();
  });

  it("keeps the material, and Escape cancels the confirmation first", async () => {
    const { user, onClose } = open(seeded(C25));
    await ask(user);
    await user.click(screen.getByRole("button", { name: "Keep material" }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(screen.getByRole("listbox", { name: "Project materials" })).toHaveFocus();
    await user.keyboard("{Delete}");
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("explains a material in use instead of confirming", async () => {
    const core = seeded(C25);
    core.materials.setUsedBy("C25/30", 3);
    const { user } = open(core);
    await ask(user);
    expect(
      await screen.findByText(
        "3 build-up layers use C25/30. Change their material in the build-ups, then delete it.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  it("uses the singular for one layer", async () => {
    const core = seeded(C25);
    core.materials.setUsedBy("C25/30", 1);
    const { user } = open(core);
    await ask(user);
    expect(await screen.findByText(/^1 build-up layer uses C25\/30\./)).toBeInTheDocument();
  });

  it("explains it too when the core says in_use after all", async () => {
    const core = seeded(C25);
    core.override("materials.delete", () => {
      throw new ApiError(-32000, "refused", { reason: "in_use", used_by: 2 });
    });
    const { user } = open(core);
    await ask(user);
    await user.click(screen.getByRole("button", { name: "Delete material" }));
    expect(await screen.findByText(/^2 build-up layers use C25\/30\./)).toBeInTheDocument();
  });

  it("falls back to one use when in_use carries no count, and shows a banner for any other failure", async () => {
    const core = seeded(C25);
    const { user } = open(core);
    core.override("materials.delete", () => {
      throw refusal("in_use");
    });
    await ask(user);
    await user.click(screen.getByRole("button", { name: "Delete material" }));
    expect(await screen.findByText(/^1 build-up layer uses/)).toBeInTheDocument();
    core.override("materials.delete", () => {
      throw refusal("not_found");
    });
    await ask(user);
    await user.click(screen.getByRole("button", { name: "Delete material" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("This material no longer exists.");
  });
});

describe("adding while the list is filtered", () => {
  it("selects the new material and marks the grades of the whole project in the library", async () => {
    const { user } = open(seeded(S355));
    await user.type(await screen.findByRole("searchbox"), "s35");
    await user.click(screen.getByRole("button", { name: "Add from library…" }));
    expect(await grade(/^S355/)).toHaveTextContent("In the project");
    await user.click(await grade(/^C25\/30/));
    await user.click(screen.getByRole("button", { name: "Add to project" }));
    expect(await screen.findByRole("heading", { name: "C25/30" })).toBeInTheDocument();
    expect(await option(/^C25\/30/)).toHaveAttribute("aria-selected", "true");
  });

  it("keeps the toast's Undo in the dialog's Tab loop", async () => {
    const { user } = open(seeded(S355));
    await user.click(await screen.findByRole("button", { name: "Add from library…" }));
    await user.click(await grade(/^C25\/30/));
    await user.click(screen.getByRole("button", { name: "Add to project" }));
    const undo = await screen.findByRole("button", { name: "Undo" });
    screen.getByRole("button", { name: "Done" }).focus();
    await user.tab();
    expect(undo).toHaveFocus();
    await user.tab();
    expect(screen.getByRole("searchbox")).toHaveFocus();
    await user.tab({ shift: true });
    expect(undo).toHaveFocus();
  });
});

describe("the library", () => {
  it("goes back to the material with a back link, and with Escape", async () => {
    const { user, onClose } = open(seeded(C25));
    await user.click(await screen.findByRole("button", { name: "Add from library…" }));
    expect(screen.getByRole("textbox", { name: "Search the library" })).toHaveFocus();
    await user.click(screen.getByRole("button", { name: "← Back to C25/30" }));
    expect(await screen.findByRole("heading", { name: "C25/30" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Add from library…" }));
    await user.keyboard("{Escape}");
    expect(await screen.findByRole("heading", { name: "C25/30" })).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalled();
  });

  it("searches, filters by category, and marks the grades already in the project", async () => {
    const { user } = open(seeded(C25));
    await user.click(await screen.findByRole("button", { name: "Add from library…" }));
    expect(await grade(/^C25\/30/)).toHaveTextContent("EN 1992-1-1 · In the project");
    expect(await grade(/^S355/)).toHaveTextContent("EN 1993-1-1");
    await user.click(screen.getByRole("button", { name: "Steel" }));
    expect(screen.getByRole("button", { name: /Steel/ })).toHaveAttribute("aria-pressed", "true");
    const library = within(screen.getByRole("listbox", { name: "Library grades" }));
    await waitFor(() =>
      expect(library.queryByRole("option", { name: /C25/ })).not.toBeInTheDocument(),
    );
    await user.click(screen.getByRole("button", { name: /Steel/ }));
    expect(await grade(/^C25\/30/)).toBeInTheDocument();
    await user.type(screen.getByRole("textbox", { name: "Search the library" }), "zzz");
    expect(await screen.findByText("No grade matches “zzz”.")).toBeInTheDocument();
  });

  it("previews the values of a grade, with the arrows moving between grades", async () => {
    const { user } = open(seeded(S355));
    await user.click(await screen.findByRole("button", { name: "Add from library…" }));
    await user.click(await grade(/^C25\/30/));
    expect(await screen.findByRole("heading", { name: "Values of C25/30" })).toBeInTheDocument();
    expect(screen.getByText("25 MPa")).toBeInTheDocument();
    screen.getByRole("listbox", { name: "Library grades" }).focus();
    await user.keyboard("{ArrowDown}");
    expect(await grade(/^C30\/37/)).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{ArrowUp}{ArrowUp}");
    expect(await grade(/^C25\/30/)).toHaveAttribute("aria-selected", "true");
    await user.keyboard("a");
  });

  it("shows more grades when the library has more than one page", async () => {
    const core = seeded(C25);
    const items = [
      { key: C25, name: "C25/30", category: "concrete", design_code: "en1992-1-1" },
      { key: S355, name: "S355", category: "steel", design_code: null },
    ];
    core.override("materials.library", (params) => {
      const { offset = 0 } = params as { offset?: number };
      return { total: 2, items: offset === 0 ? items.slice(0, 1) : items.slice(1) };
    });
    const { user } = open(core);
    await user.click(await screen.findByRole("button", { name: "Add from library…" }));
    await grade(/^C25/);
    await user.click(screen.getByRole("button", { name: "Show more grades" }));
    expect(await grade(/^S355/)).toHaveTextContent("S355");
    expect(screen.queryByRole("button", { name: "Show more grades" })).not.toBeInTheDocument();
  });

  it("says so when the library cannot be read, or a grade has gone", async () => {
    const core = seeded(C25);
    const { user } = open(core);
    await user.click(await screen.findByRole("button", { name: "Add from library…" }));
    await grade(/^C30/);
    core.failNext("materials.library", new Error("down"));
    await user.type(screen.getByRole("textbox", { name: "Search the library" }), "c");
    expect(await screen.findByText("The library could not be read.")).toBeInTheDocument();

    await user.clear(screen.getByRole("textbox", { name: "Search the library" }));
    await user.click(await grade(/^C30/));
    core.override("materials.add_from_library", () => {
      throw refusal("unknown_grade");
    });
    await user.click(screen.getByRole("button", { name: "Add to project" }));
    expect(await screen.findByText("This grade is no longer in the library.")).toBeInTheDocument();
  });

  it("shows a banner when a grade cannot be read", async () => {
    const core = seeded(C25);
    core.override("materials.grade", () => {
      throw refusal("unknown_grade");
    });
    const { user } = open(core);
    await user.click(await screen.findByRole("button", { name: "Add from library…" }));
    await user.click(await grade(/^C30/));
    expect(await screen.findByText("This grade is no longer in the library.")).toBeInTheDocument();
  });
});

describe("the dialog", () => {
  it("closes with Done", async () => {
    const { user, onClose } = open(seeded(C25));
    await user.click(await screen.findByRole("button", { name: "Done" }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("is named Materials and is modal", async () => {
    open(seeded(C25));
    expect(await screen.findByRole("dialog", { name: "Materials" })).toHaveAttribute(
      "aria-modal",
      "true",
    );
  });

  it("goes back from a sub-view to the material, never closing", async () => {
    const { user, onClose } = open(seeded(C25));
    await user.click(await screen.findByRole("button", { name: "New material…" }));
    await user.keyboard("{Escape}");
    expect(await screen.findByRole("heading", { name: "C25/30" })).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });
});
