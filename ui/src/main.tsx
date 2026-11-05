// Global tokens and page styles first, so component styles can build on them.
import "./shell/theme.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { ApiClient, ApiProvider, tauriTransport } from "./api";
import { GeorefProvider } from "./georef/GeorefContext";
import { I18nProvider } from "./i18n/I18nProvider";
import { Shell } from "./shell/Shell";

const container = document.getElementById("root");
if (container === null) {
  throw new Error("index.html has no #root element");
}
const api = new ApiClient(tauriTransport);
createRoot(container).render(
  <StrictMode>
    <I18nProvider>
      <ApiProvider client={api}>
        <GeorefProvider>
          <Shell />
        </GeorefProvider>
      </ApiProvider>
    </I18nProvider>
  </StrictMode>,
);
