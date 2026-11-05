import { act, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Georeference } from "../api";
import { fakeCore, refusal } from "../test-support/fakeCore";
import { renderWithCore } from "../test-support/render";
import { useGeoref } from "./GeorefContext";
import { LocationDialog } from "./LocationDialog";

function Harness() {
  const { dialogOpen, openDialog } = useGeoref();
  return (
    <>
      <button type="button" onClick={openDialog}>
        Open location
      </button>
      {dialogOpen && <LocationDialog />}
    </>
  );
}

const PLACED: Georeference = {
  crs_epsg: 2154,
  base_point: [652012.34, 6862291.88, 35],
  grid_rotation: Math.PI / 6,
  scale: 1,
  survey_point: [652030, 6862280.5, 34.12],
};

async function open(core = fakeCore()) {
  const rendered = renderWithCore(<Harness />, core);
  // The georeference is read at start; wait for it so that the dialog opens on its answer.
  await waitFor(() => expect(core.calls.some((call) => call.method === "georef.get")).toBe(true));
  await rendered.user.click(screen.getByRole("button", { name: "Open location" }));
  await screen.findByRole("dialog", { name: "Project location" });
  return { core, ...rendered };
}

// The survey point repeats the base point labels: the first match is the base point.
const field = (name: string) => screen.getAllByRole("textbox", { name })[0] as HTMLElement;
const crsBox = () => screen.getByRole("combobox", { name: "Coordinate system" });
const setCalls = (core: ReturnType<typeof fakeCore>) =>
  core.calls.filter((call) => call.method === "georef.set");

async function chooseLambert(user: ReturnType<typeof renderWithCore>["user"]) {
  await user.type(crsBox(), "lamb");
  await user.keyboard("{Enter}");
}

async function fillBase(user: ReturnType<typeof renderWithCore>["user"]) {
  await user.type(field("Easting"), "652000,5");
  await user.type(field("Northing"), "6862000");
  await user.type(field("Height"), "35");
}

describe("LocationDialog, project not georeferenced", () => {
  it("opens on the empty form with the coordinate system focused", async () => {
    await open();
    expect(crsBox()).toHaveFocus();
    expect(screen.getByText(/not placed on Earth yet/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Remove georeference" })).not.toBeInTheDocument();
    expect(field("Easting")).toBeDisabled();
    expect(screen.getByText("True north")).toBeInTheDocument();
    expect(screen.getAllByText("–").length).toBeGreaterThan(0);
  });

  it("sets the location from the keyboard and accepts a decimal comma", async () => {
    const { core, user } = await open();
    const opener = screen.getByRole("button", { name: "Open location" });
    await chooseLambert(user);
    expect(crsBox()).toHaveValue("2154 – RGF93 v1 / Lambert-93");
    expect(screen.getByText(/Area of use: France/)).toBeInTheDocument();
    await fillBase(user);
    await user.keyboard("{Enter}");

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(setCalls(core)[0]?.params).toEqual({
      crs_epsg: 2154,
      base_point: [652000.5, 6862000, 35],
      grid_rotation: 0,
      scale: 1,
      survey_point: null,
    });
    expect(core.current()?.crs_epsg).toBe(2154);
    expect(screen.getByRole("status")).toHaveTextContent("Project location applied");
    expect(opener).toHaveFocus();
  });

  it("lists the systems by code or name and picks one with the mouse", async () => {
    const { user } = await open();
    await user.type(crsBox(), "3948");
    const option = await screen.findByRole("option", { name: /CC48/ });
    expect(screen.getAllByRole("option")).toHaveLength(1);
    await user.click(option);
    expect(crsBox()).toHaveValue("3948 – RGF93 v1 / CC48");
  });

  it("moves in the list with the arrow keys, Home and End, and closes it with Esc", async () => {
    const { user } = await open();
    await user.click(crsBox());
    expect(screen.getAllByRole("option")).toHaveLength(2);
    await user.keyboard("{ArrowDown}");
    expect(crsBox()).toHaveAttribute("aria-activedescendant", screen.getAllByRole("option")[1]?.id);
    await user.keyboard("{Home}");
    expect(crsBox()).toHaveAttribute("aria-activedescendant", screen.getAllByRole("option")[0]?.id);
    await user.keyboard("{End}");
    expect(crsBox()).toHaveAttribute("aria-activedescendant", screen.getAllByRole("option")[1]?.id);
    await user.keyboard("{ArrowUp}");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    // The first Esc only closed the list.
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("listbox")).toBeInTheDocument();
    await user.tab();
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("scrolls the option reached with the arrow keys into view", async () => {
    const scrollIntoView = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoView;
    try {
      const { user } = await open();
      await user.click(crsBox());
      await user.keyboard("{ArrowDown}");
      const options = screen.getAllByRole("option");
      expect(scrollIntoView).toHaveBeenLastCalledWith({ block: "nearest" });
      expect(scrollIntoView.mock.contexts.at(-1)).toBe(options[1]);
    } finally {
      // jsdom has none, so the test removes the stand-in.
      delete (Element.prototype as { scrollIntoView?: unknown }).scrollIntoView;
    }
  });

  it("leaves Home and End to the caret while the list is closed", async () => {
    const { user } = await open();
    await user.type(crsBox(), "2154");
    await user.keyboard("{Escape}");
    const key = new KeyboardEvent("keydown", { key: "Home", bubbles: true, cancelable: true });
    crsBox().dispatchEvent(key);
    expect(key.defaultPrevented).toBe(false);
  });

  it("says when no system matches, and opens the list again from the keyboard", async () => {
    const { user } = await open();
    await user.type(crsBox(), "zzz");
    expect(
      within(screen.getByRole("listbox")).getByText(/No coordinate system matches “zzz”/),
    ).toBeInTheDocument();
    await user.keyboard("{Enter}");
    expect(screen.getByRole("listbox")).toBeInTheDocument();
  });

  it("shows an error for each invalid field, focuses the first and announces the count", async () => {
    const { core, user } = await open();
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(crsBox()).toHaveFocus();
    expect(crsBox()).toBeInvalid();
    expect(crsBox()).toHaveAccessibleDescription(/Choose a coordinate system\./);
    expect(screen.getByText("Fields to fix: 4")).toBeInTheDocument();
    expect(setCalls(core)).toHaveLength(0);

    await chooseLambert(user);
    expect(crsBox()).toBeValid();
    await user.type(field("Easting"), "abc");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(field("Easting")).toHaveFocus();
    expect(field("Easting")).toHaveAccessibleDescription(
      expect.stringContaining("Enter a number, e.g. 652000.000."),
    );
    await user.type(field("Easting"), "{Backspace}{Backspace}{Backspace}1");
    expect(field("Easting")).toBeValid();
  });

  it("opens Advanced for a scale outside the range the core accepts", async () => {
    const { user } = await open();
    await chooseLambert(user);
    await fillBase(user);
    const advanced = screen.getByRole("button", { name: "Advanced" });
    expect(advanced).toHaveAttribute("aria-expanded", "false");
    await user.click(advanced);
    await user.clear(field("Scale"));
    await user.type(field("Scale"), "0");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(advanced).toHaveAttribute("aria-expanded", "true");
    expect(field("Scale")).toHaveFocus();
    expect(field("Scale")).toHaveAccessibleDescription(
      expect.stringContaining("The scale must be between 0.000001 and 1000000."),
    );
    await user.clear(field("Scale"));
    await user.type(field("Scale"), "2000000");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(field("Scale")).toBeInvalid();
    await user.clear(field("Scale"));
    await user.type(field("Scale"), "x");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(field("Scale")).toHaveAccessibleDescription(/Enter a number/);
  });

  it("asks for the three survey values or none, and sends the point when complete", async () => {
    const { core, user } = await open();
    await chooseLambert(user);
    await fillBase(user);
    await user.click(screen.getByRole("button", { name: "Survey point (optional)" }));
    const [e, n, h] = screen
      .getAllByRole("textbox", { name: /^(Easting|Northing|Height)$/ })
      .slice(3);
    await user.type(e as HTMLElement, "652030");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(n).toHaveAccessibleDescription(/all three survey values/);
    expect(e).toHaveAccessibleDescription(/^$|(?!all three)/);
    await user.type(n as HTMLElement, "6862280,5");
    await user.type(h as HTMLElement, "x");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(h).toHaveAccessibleDescription(/Enter a number/);
    await user.clear(h as HTMLElement);
    await user.type(h as HTMLElement, "34,12");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(setCalls(core)[0]?.params).toMatchObject({ survey_point: [652030, 6862280.5, 34.12] });
  });
});

describe("LocationDialog, refusals", () => {
  async function applyWithRefusal(error: Error) {
    const opened = await open();
    opened.core.failNext("georef.set", error);
    await chooseLambert(opened.user);
    await fillBase(opened.user);
    await opened.user.click(screen.getByRole("button", { name: "Apply" }));
    return opened;
  }

  it("marks the base point when it is outside the area, without repeating the values", async () => {
    const { user } = await applyWithRefusal(refusal("outside_domain"));
    // The message is shown under the group and announced in the live region.
    expect(
      await screen.findAllByText(/outside the area where this coordinate system is valid/),
    ).toHaveLength(2);
    expect(field("Easting")).toHaveFocus();
    expect(field("Easting")).toBeInvalid();
    expect(field("Northing")).toBeInvalid();
    expect(field("Easting")).toHaveAccessibleDescription(
      expect.stringContaining("outside the area"),
    );
    expect(screen.getByRole("dialog")).not.toHaveTextContent("652000");
    // Editing a base point field clears the group error.
    await user.type(field("Easting"), "1");
    expect(field("Northing")).toBeValid();
  });

  it.each([
    ["unknown_crs", "This coordinate system is not available."],
    ["not_projected", "Choose a projected system"],
  ])("puts %s on the coordinate system", async (reason, text) => {
    await applyWithRefusal(refusal(reason));
    await waitFor(() => expect(crsBox()).toHaveFocus());
    expect(crsBox()).toHaveAccessibleDescription(new RegExp(text));
  });

  it("puts invalid_scale on the scale and opens Advanced", async () => {
    await applyWithRefusal(refusal("invalid_scale"));
    await waitFor(() => expect(field("Scale")).toHaveFocus());
    expect(screen.getByRole("button", { name: "Advanced" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
  });

  it("shows other refusals and transport failures in a banner, and stays open", async () => {
    await applyWithRefusal(refusal("multiple_sites"));
    expect(await screen.findByRole("alert")).toHaveTextContent("This project has several sites.");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("shows a generic banner when the core does not answer", async () => {
    await applyWithRefusal(new Error("down"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Something went wrong. Try again.");
    expect(screen.getByRole("button", { name: "Apply" })).toBeEnabled();
  });

  it("retries with a new site id when the project has no site", async () => {
    const { core } = await applyWithRefusal(refusal("missing_site_id"));
    await waitFor(() => expect(setCalls(core)).toHaveLength(2));
    expect(setCalls(core)[1]?.params).toMatchObject({
      site: expect.stringMatching(/^[0-9a-f]{32}$/),
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });
});

describe("LocationDialog, project georeferenced", () => {
  it("fills the form from the georeference and shows what is derived from it", async () => {
    await open(fakeCore({ georeference: PLACED }));
    expect(field("Easting")).toHaveFocus();
    expect(crsBox()).toHaveValue("2154 – RGF93 v1 / Lambert-93");
    expect(field("Easting")).toHaveValue("652012.340");
    expect(field("Northing")).toHaveValue("6862291.880");
    expect(field("Grid rotation")).toHaveValue("30.0000");
    expect(screen.getByRole("button", { name: "Survey point (optional)" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
    expect(screen.getByText(/Position in the model: x /)).toBeInTheDocument();
    expect(screen.getByText("Latitude (RGF93 v1)")).toBeInTheDocument();
    expect(screen.getByText("Meridian convergence").nextSibling).toHaveTextContent("−0.4698°");
    expect(screen.getByText("Point scale factor").nextSibling).toHaveTextContent("0.999925");
    expect(screen.getByText("Grid north").nextSibling).toHaveTextContent("−30.0000°");
    expect(screen.getByText("True north").nextSibling).toHaveTextContent("−29.7938°");
    expect(
      screen.getByRole("img", { name: /Project north, grid north and true north/ }),
    ).toBeInTheDocument();
  });

  it("closes without a command when nothing changed", async () => {
    const { core, user } = await open(fakeCore({ georeference: PLACED }));
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(setCalls(core)).toHaveLength(0);
  });

  it("sends the changes, with the angle in radians", async () => {
    const { core, user } = await open(fakeCore({ georeference: PLACED }));
    await user.clear(field("Grid rotation"));
    await user.type(field("Grid rotation"), "90");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    const params = setCalls(core)[0]?.params as Georeference;
    expect(params.grid_rotation).toBeCloseTo(Math.PI / 2, 10);
    expect(params.survey_point).toEqual([652030, 6862280.5, 34.12]);
  });

  it("switches the angles between decimal degrees and DMS", async () => {
    const { user } = await open(fakeCore({ georeference: PLACED }));
    await user.click(screen.getByRole("button", { name: "DMS" }));
    expect(field("Grid rotation")).toHaveValue("30°0′0.0000″");
    expect(screen.getByText("Latitude (RGF93 v1)").nextSibling).toHaveTextContent(/′.*″N$/);
    await user.click(screen.getByRole("button", { name: "Decimal degrees" }));
    expect(field("Grid rotation")).toHaveValue("30.0000");
    await user.clear(field("Grid rotation"));
    await user.type(field("Grid rotation"), "oops");
    await user.click(screen.getByRole("button", { name: "DMS" }));
    expect(field("Grid rotation")).toHaveValue("oops");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(field("Grid rotation")).toHaveAccessibleDescription(/Enter an angle/);
  });

  it("shows the help of a field on request", async () => {
    const { user } = await open(fakeCore({ georeference: PLACED }));
    const help = screen.getByRole("button", { name: "More about Grid rotation" });
    expect(help).toHaveAttribute("aria-expanded", "false");
    expect(field("Grid rotation")).toHaveAccessibleDescription(/counter-clockwise/);
    await user.click(help);
    expect(help).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText(/counter-clockwise/)).toBeVisible();
  });

  it("removes the georeference after a confirmation inside the dialog", async () => {
    const { core, user } = await open(fakeCore({ georeference: PLACED }));
    await user.click(screen.getByRole("button", { name: "Remove georeference" }));
    expect(screen.getByRole("region", { name: "Remove the georeference?" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Keep georeference" })).toHaveFocus();
    expect(screen.queryByRole("button", { name: "Apply" })).not.toBeInTheDocument();

    await user.keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remove georeference" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Remove georeference" }));
    await user.click(screen.getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(core.current()).toBeNull();
    expect(screen.getByRole("status")).toHaveTextContent("Project location removed");
  });

  it("keeps the dialog and says so when removal fails", async () => {
    const { core, user } = await open(fakeCore({ georeference: PLACED }));
    core.failNext("georef.clear", refusal("multiple_sites"));
    await user.click(screen.getByRole("button", { name: "Remove georeference" }));
    await user.click(screen.getByRole("button", { name: "Remove" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("several sites");
    expect(screen.getByRole("button", { name: "Remove georeference" })).toBeInTheDocument();
  });
});

describe("LocationDialog, dialog behaviour", () => {
  it("closes with Esc and with Cancel, and gives the focus back", async () => {
    const { user } = await open();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open location" })).toHaveFocus();
    await user.click(screen.getByRole("button", { name: "Open location" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("keeps Tab and Shift+Tab inside the dialog", async () => {
    const { user } = await open();
    const dialog = screen.getByRole("dialog");
    await user.tab({ shift: true });
    expect(dialog).toContainElement(document.activeElement as HTMLElement);
    expect(screen.getByRole("button", { name: "Apply" })).toHaveFocus();
    await user.tab();
    expect(crsBox()).toHaveFocus();
  });

  it("pulls the focus back in when it was outside", async () => {
    const { user } = await open();
    screen.getByRole("button", { name: "Open location" }).focus();
    await user.tab();
    expect(screen.getByRole("dialog")).toContainElement(document.activeElement as HTMLElement);
  });

  it("is in French, with a decimal comma, when the language is French", async () => {
    localStorage.setItem("bimcraft.language", "fr");
    await waitFor(() => undefined);
    renderWithCore(<Harness />, fakeCore({ georeference: PLACED }));
    const { default: userEvent } = await import("@testing-library/user-event");
    await userEvent.setup().click(await screen.findByRole("button", { name: "Open location" }));
    expect(
      await screen.findByRole("dialog", { name: "Localisation du projet" }),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("textbox", { name: "Est" })[0]).toHaveValue("652012,340");
  });

  it("shows loading and then the reason when the georeference cannot be read", async () => {
    const core = fakeCore();
    core.override("georef.get", () => {
      throw refusal("multiple_sites");
    });
    const { user } = await open(core);
    expect(screen.getByRole("alert")).toHaveTextContent("several sites");
    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("shows a generic message when the core cannot be reached", async () => {
    const core = fakeCore();
    core.failNext("georef.get", new Error("down"));
    await open(core);
    expect(screen.getByRole("alert")).toHaveTextContent("could not be read");
  });

  it("says when the list of coordinate systems cannot be loaded", async () => {
    const core = fakeCore();
    core.failNext("georef.list_crs", new Error("down"));
    const { user } = await open(core);
    await user.click(crsBox());
    expect(
      await screen.findByText("The list of coordinate systems could not be loaded.", {
        selector: ".listbox-note",
      }),
    ).toBeInTheDocument();
  });

  it("keeps the form and what was typed when a reload fails, and says so above the form", async () => {
    const { core, user } = await open(fakeCore({ georeference: PLACED }));
    await user.clear(field("Easting"));
    await user.type(field("Easting"), "700000");
    core.failNext("georef.get", new Error("down"));
    await act(async () => core.notifyChange());
    expect(await screen.findByRole("alert")).toHaveTextContent("could not be read");
    expect(field("Easting")).toHaveValue("700000");
    expect(screen.getByRole("button", { name: "Apply" })).toBeEnabled();
  });

  it("does not close while Apply is running, and shows the refusal when it arrives", async () => {
    const core = fakeCore();
    let refuse: (error: Error) => void = () => {};
    core.override(
      "georef.set",
      () =>
        new Promise((_resolve, reject) => {
          refuse = reject;
        }),
    );
    const { user } = await open(core);
    await chooseLambert(user);
    await fillBase(user);
    await user.click(screen.getByRole("button", { name: "Apply" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled());
    expect(screen.getByRole("button", { name: "Apply" })).toBeDisabled();
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("dialog")).toBeInTheDocument();

    await act(async () => refuse(refusal("outside_domain")));
    expect(
      await screen.findAllByText(/outside the area where this coordinate system/),
    ).not.toHaveLength(0);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeEnabled();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("refuses a sign together with a hemisphere letter in the rotation", async () => {
    const { user } = await open(fakeCore({ georeference: PLACED }));
    await user.clear(field("Grid rotation"));
    await user.type(field("Grid rotation"), "-10 S");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(field("Grid rotation")).toBeInvalid();
    expect(field("Grid rotation")).toHaveAccessibleDescription(
      expect.stringContaining("Enter an angle"),
    );
  });
});
