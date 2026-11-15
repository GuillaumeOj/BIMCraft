import { render } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactElement } from "react";
import type { UpdatesApi } from "../api";
import { ApiProvider } from "../api";
import { GeorefProvider } from "../georef/GeorefContext";
import { I18nProvider } from "../i18n/I18nProvider";
import { ShellStateProvider } from "../shell/ShellState";
import { UpdatesProvider } from "../updates/UpdatesContext";
import type { fakeCore } from "./fakeCore";

/** What `appInfo` reports without the Tauri IPC: nothing can be updated, so no update UI. */
const NO_UPDATES: UpdatesApi = {
  appInfo: async () => ({ version: "0.0.0-dev", updatable: false }),
  checkForUpdate: async () => null,
  downloadUpdate: async () => undefined,
  installUpdate: async () => undefined,
};

/** Renders `ui` with the providers the app has: language, API client, updates and georeference. */
export function renderWithCore(
  ui: ReactElement,
  core: ReturnType<typeof fakeCore>,
  updates: UpdatesApi = NO_UPDATES,
) {
  const user = userEvent.setup();
  const result = render(ui, {
    wrapper: ({ children }) => (
      <I18nProvider>
        <ApiProvider client={core.client}>
          <ShellStateProvider>
            <UpdatesProvider api={updates}>
              <GeorefProvider>{children}</GeorefProvider>
            </UpdatesProvider>
          </ShellStateProvider>
        </ApiProvider>
      </I18nProvider>
    ),
  });
  return { user, ...result };
}
