import { render } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactElement } from "react";
import { ApiProvider } from "../api";
import { GeorefProvider } from "../georef/GeorefContext";
import { I18nProvider } from "../i18n/I18nProvider";
import type { fakeCore } from "./fakeCore";

/** Renders `ui` with the providers the app has: language, API client and georeference. */
export function renderWithCore(ui: ReactElement, core: ReturnType<typeof fakeCore>) {
  const user = userEvent.setup();
  const result = render(ui, {
    wrapper: ({ children }) => (
      <I18nProvider>
        <ApiProvider client={core.client}>
          <GeorefProvider>{children}</GeorefProvider>
        </ApiProvider>
      </I18nProvider>
    ),
  });
  return { user, ...result };
}
