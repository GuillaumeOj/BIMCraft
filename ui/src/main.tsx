import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { I18nProvider } from "./i18n/I18nProvider";
import { Shell } from "./shell/Shell";

const container = document.getElementById("root");
if (container === null) {
  throw new Error("index.html has no #root element");
}
createRoot(container).render(
  <StrictMode>
    <I18nProvider>
      <Shell />
    </I18nProvider>
  </StrictMode>,
);
