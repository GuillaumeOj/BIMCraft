import { type KeyboardEvent, useCallback, useEffect, useId, useRef, useState } from "react";
import { CoordinatesPanel } from "../georef/CoordinatesPanel";
import { useGeoref } from "../georef/GeorefContext";
import { LocationDialog } from "../georef/LocationDialog";
import { useI18n } from "../i18n/I18nProvider";
import { LanguageSelect } from "../i18n/LanguageSelect";
import { MaterialsDialog } from "../materials/MaterialsDialog";
import { useShellState } from "./ShellState";
import "./shell.css";

const RIGHT_PANELS = ["properties", "history", "coordinates"] as const;
type RightPanel = (typeof RIGHT_PANELS)[number];

/**
 * The region F6 (or Shift+F6 when `backwards`) moves to from the one holding focus. Regions are
 * the elements marked `data-region`, in document order.
 */
function nextRegion(backwards: boolean): HTMLElement | undefined {
  const regions = [...document.querySelectorAll<HTMLElement>("[data-region]")];
  const current = regions.findIndex((region) => region.contains(document.activeElement));
  if (current === -1) return backwards ? regions.at(-1) : regions[0];
  const step = backwards ? -1 : 1;
  return regions[(current + step + regions.length) % regions.length];
}

// Ignores auto-repeat so that holding the keys does not toggle the console back and forth.
const isConsoleShortcut = (event: globalThis.KeyboardEvent) =>
  event.ctrlKey && !event.altKey && !event.metaKey && !event.repeat && event.key === "`";

// Ctrl+Shift+K (Cmd+Shift+K on macOS) shows the Coordinates panel.
const isCoordinatesShortcut = (event: globalThis.KeyboardEvent) =>
  (event.ctrlKey || event.metaKey) &&
  event.shiftKey &&
  !event.altKey &&
  event.key.toLowerCase() === "k";

// Ctrl+Shift+M (Cmd+Shift+M on macOS) opens the Materials dialog.
const isMaterialsShortcut = (event: globalThis.KeyboardEvent) =>
  (event.ctrlKey || event.metaKey) &&
  event.shiftKey &&
  !event.altKey &&
  event.key.toLowerCase() === "m";

/**
 * The empty application shell (UI-02, UI-06): title bar, viewport and the panel slots, from the
 * "Workspace" boards of the FigCraft design. F6 and Shift+F6 move focus between the regions;
 * Ctrl+` opens and closes the console, Ctrl+Shift+K shows the Coordinates panel and
 * Ctrl+Shift+L opens the Project location dialog and Ctrl+Shift+M the Materials dialog.
 */
export function Shell() {
  const { t } = useI18n();
  const ids = useId();
  const [rightPanel, setRightPanel] = useState<RightPanel>("properties");
  const { dialogOpen, openDialog } = useGeoref();
  const { modalOpen } = useShellState();
  const [consoleOpen, setConsoleOpen] = useState(false);
  const [materialsOpen, setMaterialsOpen] = useState(false);
  const viewport = useRef<HTMLElement>(null);
  const tabs = useRef<Partial<Record<RightPanel, HTMLButtonElement | null>>>({});

  // The console takes focus when it opens and hands it to the viewport when it closes.
  const consoleRef = useCallback((element: HTMLElement) => {
    element.focus();
    return () => viewport.current?.focus();
  }, []);

  const selectTab = useCallback((panel: RightPanel) => {
    setRightPanel(panel);
    tabs.current[panel]?.focus();
  }, []);

  useEffect(() => {
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      // A modal dialog keeps the keyboard: these shortcuts would move focus behind it.
      if (modalOpen) return;
      if (isConsoleShortcut(event)) {
        event.preventDefault();
        setConsoleOpen((open) => !open);
      } else if (isCoordinatesShortcut(event)) {
        event.preventDefault();
        selectTab("coordinates");
      } else if (isMaterialsShortcut(event)) {
        event.preventDefault();
        setMaterialsOpen(true);
      } else if (event.key === "F6") {
        event.preventDefault();
        nextRegion(event.shiftKey)?.focus();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [selectTab, modalOpen]);

  const onTabKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    const at = RIGHT_PANELS.indexOf(rightPanel);
    const last = RIGHT_PANELS.length - 1;
    const targets: Partial<Record<string, RightPanel>> = {
      ArrowRight: RIGHT_PANELS[at === last ? 0 : at + 1],
      ArrowLeft: RIGHT_PANELS[at === 0 ? last : at - 1],
      Home: RIGHT_PANELS[0],
      End: RIGHT_PANELS[last],
    };
    const target = targets[event.key];
    if (target === undefined) return;
    event.preventDefault();
    selectTab(target);
  };

  const tabId = (panel: RightPanel) => `${ids}-tab-${panel}`;

  return (
    <div className="shell">
      <a
        className="skip-link"
        href={`#${ids}-viewport`}
        onClick={(event) => {
          // Focus directly instead of navigating to the fragment: the URL stays clean, and not
          // every webview moves focus to a fragment target.
          event.preventDefault();
          viewport.current?.focus();
        }}
      >
        {t("shell.skipToViewport")}
      </a>
      <header className="title-bar" data-region tabIndex={-1}>
        <span className="app-tile" aria-hidden="true">
          {t("app.monogram")}
        </span>
        <h1 className="app-name">{t("app.name")}</h1>
        <nav className="open-views" aria-label={t("shell.openViews")}>
          <span className="empty">{t("shell.noOpenViews")}</span>
        </nav>
        <button
          type="button"
          className="ghost-button"
          aria-haspopup="dialog"
          aria-keyshortcuts="Control+Shift+M Meta+Shift+M"
          onClick={() => setMaterialsOpen(true)}
        >
          {t("materials.open")}
        </button>
        <button
          type="button"
          className="ghost-button"
          aria-haspopup="dialog"
          aria-keyshortcuts="Control+Shift+L Meta+Shift+L"
          onClick={openDialog}
        >
          {t("georef.open")}
        </button>
        <button
          type="button"
          className="ghost-button"
          aria-pressed={consoleOpen}
          aria-keyshortcuts="Control+`"
          onClick={() => setConsoleOpen((open) => !open)}
        >
          {t("panel.console")}
        </button>
        <LanguageSelect />
      </header>
      <div className="workspace">
        <section
          className="panel panel-left"
          aria-labelledby={`${ids}-project`}
          data-region
          tabIndex={-1}
        >
          <h2 className="panel-header" id={`${ids}-project`}>
            {t("panel.project")}
          </h2>
          <p className="empty">{t("panel.projectEmpty")}</p>
        </section>
        {/* After Project so that the DOM order is the visual and F6 order. */}
        <main
          id={`${ids}-viewport`}
          className="viewport"
          aria-label={t("shell.viewport")}
          ref={viewport}
          data-region
          tabIndex={-1}
        />
        <section
          className="panel panel-right"
          aria-labelledby={tabId(rightPanel)}
          data-region
          tabIndex={-1}
        >
          <div className="panel-header" role="tablist">
            {RIGHT_PANELS.map((panel) => (
              <button
                key={panel}
                type="button"
                role="tab"
                id={tabId(panel)}
                className="panel-tab"
                aria-selected={panel === rightPanel}
                aria-controls={`${ids}-tabpanel`}
                tabIndex={panel === rightPanel ? 0 : -1}
                ref={(element) => {
                  tabs.current[panel] = element;
                }}
                onClick={() => setRightPanel(panel)}
                onKeyDown={onTabKeyDown}
              >
                {t(`panel.${panel}`)}
              </button>
            ))}
          </div>
          <div id={`${ids}-tabpanel`} role="tabpanel" aria-labelledby={tabId(rightPanel)}>
            {rightPanel === "coordinates" ? (
              <CoordinatesPanel />
            ) : (
              <p className="empty">{t(`panel.${rightPanel}Empty`)}</p>
            )}
          </div>
        </section>
        {consoleOpen && (
          <section
            className="panel panel-console"
            aria-labelledby={`${ids}-console`}
            ref={consoleRef}
            data-region
            tabIndex={-1}
          >
            <div className="panel-header">
              <h2 id={`${ids}-console`}>{t("panel.console")}</h2>
              <button
                type="button"
                className="ghost-button"
                aria-label={t("panel.closeConsole")}
                onClick={() => setConsoleOpen(false)}
              >
                <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
                  <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.5" />
                </svg>
              </button>
            </div>
            <p className="empty">{t("panel.consoleEmpty")}</p>
          </section>
        )}
      </div>
      {dialogOpen && <LocationDialog />}
      {materialsOpen && <MaterialsDialog onClose={() => setMaterialsOpen(false)} />}
    </div>
  );
}
