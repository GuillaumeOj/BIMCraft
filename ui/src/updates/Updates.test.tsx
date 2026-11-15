import { act, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { UpdateError, type UpdateOffer, type UpdateProgress, type UpdatesApi } from "../api";
import { Shell } from "../shell/Shell";
import { fakeCore } from "../test-support/fakeCore";
import { renderWithCore } from "../test-support/render";
import { AUTO_CHECK_STORAGE_KEY, CHANNEL_STORAGE_KEY } from "./settings";

const OFFER: UpdateOffer = {
  version: "0.4.0",
  date: "2026-11-12T12:00:00Z",
  notes: "- Faster <b>viewport</b>\n- Fixes",
};

type Fake = UpdatesApi & {
  /** Reports progress of the running download, then settles it. */
  progress: (progress: UpdateProgress) => void;
  finishDownload: () => void;
  failDownload: (code: ConstructorParameters<typeof UpdateError>[0]) => void;
};

function fakeUpdates(version = "0.3.0", updatable = true): Fake {
  let report: (progress: UpdateProgress) => void = () => undefined;
  let settle: { resolve: () => void; reject: (reason: unknown) => void } = {
    resolve: () => undefined,
    reject: () => undefined,
  };
  return {
    appInfo: vi.fn(async () => ({ version, updatable })),
    checkForUpdate: vi.fn(async () => OFFER),
    downloadUpdate: vi.fn(
      (onProgress) =>
        new Promise<void>((resolve, reject) => {
          report = onProgress;
          settle = { resolve: () => resolve(), reject };
        }),
    ),
    installUpdate: vi.fn(() => new Promise<void>(() => undefined)),
    progress: (progress) => act(() => report(progress)),
    finishDownload: () => act(() => settle.resolve()),
    failDownload: (code) => act(() => settle.reject(new UpdateError(code, "host"))),
  };
}

const rejecting = (code: ConstructorParameters<typeof UpdateError>[0]) =>
  vi.fn(async () => {
    throw new UpdateError(code, "host message");
  });

async function renderApp(api: UpdatesApi) {
  const view = renderWithCore(<Shell />, fakeCore(), api);
  await waitFor(() => expect(api.appInfo).toHaveBeenCalled());
  return view;
}

const dialog = () => screen.getByRole("dialog");
// The X and the footer both say Close: the footer button, the last one, is the action.
const inDialog = (name: string | RegExp) =>
  within(dialog()).getAllByRole("button", { name }).at(-1) as HTMLElement;
const openDialog = async (user: Awaited<ReturnType<typeof renderApp>>["user"]) => {
  await user.click(await screen.findByRole("button", { name: "Updates" }));
  return dialog();
};

beforeEach(() => localStorage.setItem(AUTO_CHECK_STORAGE_KEY, "off"));
afterEach(() => {
  localStorage.clear();
  vi.useRealTimers();
});

describe("when the build cannot update", () => {
  it("shows nothing: no entry, no shortcut, no dialog, no announcement", async () => {
    const { user } = await renderApp(fakeUpdates("0.0.0-dev", false));
    expect(screen.queryByRole("button", { name: /update/i })).not.toBeInTheDocument();
    await user.keyboard("{Control>}{Shift>}U{/Shift}{/Control}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});

describe("the Updates dialog", () => {
  it("opens from the title bar on Idle with Check now focused, without checking", async () => {
    const api = fakeUpdates();
    const { user } = await renderApp(api);
    await openDialog(user);
    expect(screen.getByRole("heading", { name: "Updates" })).toBeInTheDocument();
    expect(screen.getByText("You are running Fabrilo 0.3.0.")).toBeInTheDocument();
    expect(screen.getByText("Select Check now to look for a newer version.")).toBeInTheDocument();
    expect(inDialog("Check now")).toHaveFocus();
    expect(api.checkForUpdate).not.toHaveBeenCalled();
  });

  it("opens with Ctrl+Shift+U (Cmd on macOS), and Escape closes it and returns focus", async () => {
    const { user } = await renderApp(fakeUpdates());
    await screen.findByRole("button", { name: "Updates" });
    await user.keyboard("{Control>}{Shift>}U{/Shift}{/Control}");
    expect(dialog()).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await user.keyboard("{Meta>}{Shift>}U{/Shift}{/Meta}");
    expect(dialog()).toBeInTheDocument();
    await user.click(inDialog("Close"));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Updates" }));
    await user.keyboard("{Escape}");
    expect(screen.getByRole("button", { name: "Updates" })).toHaveFocus();
  });

  it("declares the shortcut and the dialog on the entry", async () => {
    await renderApp(fakeUpdates());
    const button = await screen.findByRole("button", { name: "Updates" });
    expect(button).toHaveAttribute("aria-haspopup", "dialog");
    expect(button).toHaveAttribute("aria-keyshortcuts", "Control+Shift+U Meta+Shift+U");
  });

  it("says that the version is up to date, with the time, and focuses Close", async () => {
    const api = fakeUpdates();
    vi.mocked(api.checkForUpdate).mockResolvedValue(null);
    const { user } = await renderApp(api);
    await openDialog(user);
    await user.click(inDialog("Check now"));
    expect(await screen.findByText("Fabrilo 0.3.0 is up to date.")).toBeInTheDocument();
    expect(screen.getByText(/^Last checked .+\.$/)).toBeInTheDocument();
    expect(api.checkForUpdate).toHaveBeenCalledWith("stable");
    expect(inDialog("Close")).toHaveFocus();
  });

  it("shows Checking with Check now disabled and the channel locked", async () => {
    const api = fakeUpdates();
    vi.mocked(api.checkForUpdate).mockReturnValue(new Promise(() => undefined));
    const { user } = await renderApp(api);
    await openDialog(user);
    await user.click(inDialog("Check now"));
    expect(screen.getByText("Checking for updates…")).toBeInTheDocument();
    expect(inDialog("Check now")).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByRole("group", { name: "Update channel" })).toBeDisabled();
    expect(
      screen.getByText(/The channel cannot change while an update is being checked/),
    ).toBeInTheDocument();
    await user.click(inDialog("Check now"));
    expect(api.checkForUpdate).toHaveBeenCalledTimes(1);
  });

  it("offers an update: date, plain-text release notes, Download focused", async () => {
    const { user } = await renderApp(fakeUpdates());
    await openDialog(user);
    await user.click(inDialog("Check now"));
    expect(await screen.findByText("Fabrilo 0.4.0 is available.")).toBeInTheDocument();
    expect(screen.getByText("Released Nov 12, 2026.")).toBeInTheDocument();
    const notes = screen.getByRole("region", { name: "What's new in 0.4.0" });
    expect(notes).toHaveTextContent("- Faster <b>viewport</b> - Fixes");
    expect(notes.querySelector("b")).toBeNull();
    expect(notes).toHaveAttribute("tabindex", "0");
    expect(inDialog("Download and install")).toHaveFocus();
  });

  it("hides the date when the manifest has none or an unreadable one, and notes when empty", async () => {
    const api = fakeUpdates();
    vi.mocked(api.checkForUpdate).mockResolvedValue({ version: "0.4.0", date: "soon", notes: "" });
    const { user } = await renderApp(api);
    await openDialog(user);
    await user.click(inDialog("Check now"));
    await screen.findByText("Fabrilo 0.4.0 is available.");
    expect(screen.queryByText(/Released/)).not.toBeInTheDocument();
    expect(screen.getByText("No release notes were provided.")).toBeInTheDocument();
  });

  it("downloads with progress, then offers the restart", async () => {
    const api = fakeUpdates();
    const { user } = await renderApp(api);
    await openDialog(user);
    await user.click(inDialog("Check now"));
    await user.click(await screen.findByRole("button", { name: "Download and install" }));
    expect(screen.getByText("Downloading Fabrilo 0.4.0…")).toBeInTheDocument();
    expect(inDialog("Close")).toHaveFocus();
    await api.progress({ downloaded: 17_000_000, total: 40_000_000 });
    const bar = screen.getByRole("progressbar", { name: "Download progress" });
    expect(bar).toHaveAttribute("aria-valuenow", "42");
    expect(bar).toHaveAttribute("aria-valuetext", "17 MB of 40 MB");
    expect(screen.getByText("42%")).toBeInTheDocument();
    await api.progress({ downloaded: 2_400_000, total: null });
    expect(bar).not.toHaveAttribute("aria-valuenow");
    expect(bar).toHaveAttribute("aria-valuetext", "2.4 MB downloaded");
    expect(screen.queryByText(/%$/)).not.toBeInTheDocument();
    await api.finishDownload();
    expect(await screen.findByText("Fabrilo 0.4.0 is ready to install.")).toBeInTheDocument();
    expect(inDialog("Restart now")).toHaveFocus();
  });

  it("keeps the download going when the dialog closes, and the entry shows it", async () => {
    const api = fakeUpdates();
    const { user } = await renderApp(api);
    await openDialog(user);
    await user.click(inDialog("Check now"));
    await user.click(await screen.findByRole("button", { name: "Download and install" }));
    await user.click(inDialog("Close"));
    expect(screen.getByRole("button", { name: "Downloading…" })).toBeInTheDocument();
    await api.progress({ downloaded: 1, total: 4 });
    expect(screen.getByRole("button", { name: "Downloading 25%" })).toBeInTheDocument();
    await api.finishDownload();
    expect(await screen.findByRole("button", { name: "Restart to update" })).toBeInTheDocument();
    expect(
      screen.getByText("Fabrilo 0.4.0 is ready to install. Open Updates to restart."),
    ).toBeInTheDocument();
  });

  it("asks before restarting: Cancel is the default, Cancel and Escape go back", async () => {
    const api = fakeUpdates();
    const { user } = await renderApp(api);
    await openDialog(user);
    await user.click(inDialog("Check now"));
    await user.click(await screen.findByRole("button", { name: "Download and install" }));
    await api.finishDownload();
    await user.click(await screen.findByRole("button", { name: "Restart now" }));
    expect(screen.getByRole("heading", { name: "Restart Fabrilo now?" })).toBeInTheDocument();
    expect(screen.getByText(/closes and reopens as version 0\.4\.0/)).toBeInTheDocument();
    expect(screen.queryByText(/recovery copy/)).not.toBeInTheDocument();
    expect(inDialog("Cancel")).toHaveFocus();
    await user.click(inDialog("Cancel"));
    expect(screen.getByText("Fabrilo 0.4.0 is ready to install.")).toBeInTheDocument();
    expect(inDialog("Restart now")).toHaveFocus();
    await user.click(inDialog("Restart now"));
    await user.keyboard("{Escape}");
    expect(screen.getByText("Fabrilo 0.4.0 is ready to install.")).toBeInTheDocument();
    expect(api.installUpdate).not.toHaveBeenCalled();
  });

  it("installs after the confirmation and shows Restarting, which Escape cannot close", async () => {
    const api = fakeUpdates();
    const { user } = await renderApp(api);
    await openDialog(user);
    await user.click(inDialog("Check now"));
    await user.click(await screen.findByRole("button", { name: "Download and install" }));
    await api.finishDownload();
    await user.click(await screen.findByRole("button", { name: "Restart now" }));
    await user.click(inDialog("Restart now"));
    expect(api.installUpdate).toHaveBeenCalledTimes(1);
    expect(screen.getByText("Restarting Fabrilo…")).toBeInTheDocument();
    expect(inDialog("Close")).toBeDisabled();
    await user.keyboard("{Escape}");
    expect(dialog()).toBeInTheDocument();
  });

  it("Later closes the dialog: the entry is the plain button again, or Restart to update", async () => {
    const api = fakeUpdates();
    const { user } = await renderApp(api);
    await openDialog(user);
    await user.click(inDialog("Check now"));
    expect(
      await screen.findByRole("button", { name: "Update available: Fabrilo 0.4.0" }),
    ).toBeInTheDocument();
    await user.click(inDialog("Later"));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Updates" }));
    expect(inDialog("Download and install")).toHaveFocus();
    await user.click(inDialog("Download and install"));
    await api.finishDownload();
    await user.click(await screen.findByRole("button", { name: "Later" }));
    expect(screen.getByRole("button", { name: "Restart to update" })).toBeInTheDocument();
  });
});

describe("errors", () => {
  const CASES = [
    ["network", /Could not reach the update server/],
    ["manifest", /could not be read right now/],
    ["unavailable", /Something went wrong with the update/],
    ["unknown", /Something went wrong with the update/],
  ] as const;

  it.each(CASES)("a failed on-demand check (%s) shows its text as an alert", async (code, text) => {
    const api = fakeUpdates();
    api.checkForUpdate = rejecting(code);
    const { user } = await renderApp(api);
    await openDialog(user);
    await user.click(inDialog("Check now"));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(text);
    expect(screen.queryByText("host message")).not.toBeInTheDocument();
    // The alert sits next to the status area, not inside it.
    expect(alert.closest("[role=status]")).toBeNull();
  });

  it("offers Try again after a network failure, which checks again", async () => {
    const api = fakeUpdates();
    vi.mocked(api.checkForUpdate).mockRejectedValueOnce(new UpdateError("network", "x"));
    const { user } = await renderApp(api);
    await openDialog(user);
    await user.click(inDialog("Check now"));
    await screen.findByRole("alert");
    expect(inDialog("Try again")).toHaveFocus();
    await user.click(inDialog("Try again"));
    expect(await screen.findByText("Fabrilo 0.4.0 is available.")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("the manifest error tells MSI installations to use Stable", async () => {
    const api = fakeUpdates();
    api.checkForUpdate = rejecting("manifest");
    const { user } = await renderApp(api);
    await openDialog(user);
    await user.click(inDialog("Check now"));
    await screen.findByRole("alert");
    expect(screen.getByText(/MSI package only receive Stable updates/)).toBeInTheDocument();
  });

  it("the 'other' failures offer only Check now", async () => {
    const api = fakeUpdates();
    api.checkForUpdate = rejecting("unavailable");
    const { user } = await renderApp(api);
    await openDialog(user);
    await user.click(inDialog("Check now"));
    await screen.findByRole("alert");
    expect(within(dialog()).queryByRole("button", { name: "Try again" })).not.toBeInTheDocument();
    expect(inDialog("Check now")).toHaveFocus();
  });

  it("a signature failure keeps the old version and Try again downloads again", async () => {
    const api = fakeUpdates();
    const { user } = await renderApp(api);
    await openDialog(user);
    await user.click(inDialog("Check now"));
    await user.click(await screen.findByRole("button", { name: "Download and install" }));
    await api.failDownload("signature");
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The downloaded update could not be verified, so it was not installed. Your current version is unchanged.",
    );
    expect(screen.getByText(/download Fabrilo again/)).toBeInTheDocument();
    expect(inDialog("Try again")).toHaveFocus();
    await user.click(inDialog("Try again"));
    expect(api.downloadUpdate).toHaveBeenCalledTimes(2);
    expect(screen.getByText("Downloading Fabrilo 0.4.0…")).toBeInTheDocument();
  });

  it("a failed download shows 'Update failed' on the entry and announces it once", async () => {
    const api = fakeUpdates();
    const { user } = await renderApp(api);
    await openDialog(user);
    await user.click(inDialog("Check now"));
    await user.click(await screen.findByRole("button", { name: "Download and install" }));
    await user.click(inDialog("Close"));
    await api.failDownload("network");
    expect(await screen.findByRole("button", { name: "Update failed" })).toBeInTheDocument();
    expect(
      screen.getByText("The update could not be downloaded. Open Updates for details."),
    ).toBeInTheDocument();
  });

  it("a download with no offer on the host asks to check again", async () => {
    const api = fakeUpdates();
    const { user } = await renderApp(api);
    await openDialog(user);
    await user.click(inDialog("Check now"));
    await user.click(await screen.findByRole("button", { name: "Download and install" }));
    await api.failDownload("no_update");
    await screen.findByRole("alert");
    await user.click(inDialog("Check now"));
    expect(api.checkForUpdate).toHaveBeenCalledTimes(2);
  });

  async function toRestartFailure(code: "install" | "not_downloaded") {
    const api = fakeUpdates();
    api.installUpdate = rejecting(code);
    const { user } = await renderApp(api);
    await openDialog(user);
    await user.click(inDialog("Check now"));
    await user.click(await screen.findByRole("button", { name: "Download and install" }));
    await api.finishDownload();
    await user.click(await screen.findByRole("button", { name: "Restart now" }));
    await user.click(inDialog("Restart now"));
    await screen.findByRole("alert");
    return { api, user };
  }

  it("an install failure stays on the Ready screen; Try again asks again", async () => {
    const { user } = await toRestartFailure("install");
    expect(screen.getByRole("alert")).toHaveTextContent("Fabrilo could not install the update.");
    expect(screen.getByText("Fabrilo 0.4.0 is ready to install.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Restart to update" })).toBeInTheDocument();
    await user.click(inDialog("Try again"));
    expect(screen.getByRole("heading", { name: "Restart Fabrilo now?" })).toBeInTheDocument();
  });

  it("a missing package goes back to downloading it again", async () => {
    const { api, user } = await toRestartFailure("not_downloaded");
    expect(screen.getByRole("alert")).toHaveTextContent("Fabrilo could not install the update.");
    await user.click(inDialog("Try again"));
    expect(api.downloadUpdate).toHaveBeenCalledTimes(2);
  });
});

describe("settings", () => {
  it("remembers the channel, checks the new channel at once and keeps it across reloads", async () => {
    const api = fakeUpdates();
    const { user, unmount } = await renderApp(api);
    await openDialog(user);
    await user.click(within(dialog()).getByRole("button", { name: "Beta" }));
    expect(api.checkForUpdate).toHaveBeenCalledWith("beta");
    expect(localStorage.getItem(CHANNEL_STORAGE_KEY)).toBe("beta");
    expect(screen.getByText(/Use Beta only if you accept occasional problems/)).toBeInTheDocument();
    unmount();
    const again = await renderApp(fakeUpdates());
    await openDialog(again.user);
    expect(within(dialog()).getByRole("button", { name: "Beta" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("locks the channel while downloading", async () => {
    const api = fakeUpdates();
    const { user } = await renderApp(api);
    await openDialog(user);
    await user.click(inDialog("Check now"));
    await user.click(await screen.findByRole("button", { name: "Download and install" }));
    expect(screen.getByRole("group", { name: "Update channel" })).toBeDisabled();
  });

  it("explains that a beta stays installed when Stable is chosen", async () => {
    const { user } = await renderApp(fakeUpdates("0.4.0-beta.1"));
    await openDialog(user);
    expect(screen.getByText(/You are running a beta. It stays installed/)).toBeInTheDocument();
    await user.click(within(dialog()).getByRole("button", { name: "Beta" }));
    expect(screen.queryByText(/You are running a beta/)).not.toBeInTheDocument();
  });

  it("remembers the automatic check switch (on by default)", async () => {
    localStorage.removeItem(AUTO_CHECK_STORAGE_KEY);
    const { user } = await renderApp(fakeUpdates());
    await openDialog(user);
    const toggle = screen.getByRole("checkbox", { name: "Check for updates automatically" });
    expect(toggle).toBeChecked();
    expect(toggle).toHaveAccessibleDescription(/only fetches a public file/);
    await user.click(toggle);
    expect(localStorage.getItem(AUTO_CHECK_STORAGE_KEY)).toBe("off");
    expect(toggle).not.toBeChecked();
  });
});

describe("the check at start-up", () => {
  beforeEach(() => {
    localStorage.removeItem(AUTO_CHECK_STORAGE_KEY);
    vi.useFakeTimers({ shouldAdvanceTime: true });
  });
  const wait = (ms: number) => act(() => vi.advanceTimersByTimeAsync(ms));

  it("checks once a few seconds after start and announces an offer without opening anything", async () => {
    const api = fakeUpdates();
    await renderApp(api);
    await wait(4000);
    expect(api.checkForUpdate).not.toHaveBeenCalled();
    await wait(1500);
    expect(api.checkForUpdate).toHaveBeenCalledWith("stable");
    expect(
      await screen.findByRole("button", { name: "Update available: Fabrilo 0.4.0" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(
      screen.getByText("Fabrilo 0.4.0 is available. Open Updates to download it."),
    ).toBeInTheDocument();
    await wait(20_000);
    expect(api.checkForUpdate).toHaveBeenCalledTimes(1);
  });

  it("is silent when it fails: plain button, no alert, dialog opens on Idle", async () => {
    const api = fakeUpdates();
    api.checkForUpdate = rejecting("network");
    const { user } = await renderApp(api);
    await wait(5500);
    expect(api.checkForUpdate).toHaveBeenCalled();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Updates" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByText("Select Check now to look for a newer version.")).toBeInTheDocument();
  });

  it("does not run when the switch is off, and turning it off cancels the pending check", async () => {
    const api = fakeUpdates();
    const { user } = await renderApp(api);
    await wait(1000);
    await user.click(await screen.findByRole("button", { name: "Updates" }));
    await user.click(screen.getByRole("checkbox"));
    await wait(10_000);
    expect(api.checkForUpdate).not.toHaveBeenCalled();
  });

  it("waits for an open dialog to close", async () => {
    const api = fakeUpdates();
    const { user } = await renderApp(api);
    await user.click(await screen.findByRole("button", { name: "Updates" }));
    await wait(6000);
    expect(api.checkForUpdate).not.toHaveBeenCalled();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(api.checkForUpdate).toHaveBeenCalledTimes(1));
  });

  it("leaves a user who already started something alone", async () => {
    const api = fakeUpdates();
    const { user } = await renderApp(api);
    await user.click(await screen.findByRole("button", { name: "Updates" }));
    await user.click(inDialog("Check now"));
    await screen.findByText("Fabrilo 0.4.0 is available.");
    await user.click(inDialog("Download and install"));
    await user.click(inDialog("Close"));
    await wait(6000);
    expect(api.checkForUpdate).toHaveBeenCalledTimes(1);
  });
});
