import { useCallback, useEffect, useRef, useState } from "react";
import { hasDrawableElements, moveElements, moveRefusal, placeBlock, useApi } from "../api";
import { useI18n } from "../i18n/I18nProvider";
import type { MessageKey } from "../i18n/i18n";
import { useShellState } from "../shell/ShellState";
import { announcementText, formatMetres, previewText } from "./format";
import { NavigationHint } from "./HintCard";
import { EmptyCard, FailedCard, LoadingCard, UnavailableCard } from "./StateCards";
import { viewportShortcut } from "./shortcuts";
import { Toolbar } from "./Toolbar";
import type {
  Intent,
  PreviewLabel,
  Projection,
  Tool,
  ToolStep,
  ViewCommand,
  ViewportEvent,
} from "./types";
import { useViewportSession } from "./useViewportSession";
import { ViewControls } from "./ViewControls";
import "./viewport.css";

/** The steps that show a hint above the tool bar. */
const HINTS: Partial<Record<ToolStep, MessageKey>> = {
  first_corner: "viewport.hint.first_corner",
  second_corner: "viewport.hint.second_corner",
  height: "viewport.hint.height",
  base_point: "viewport.hint.base_point",
  target_point: "viewport.hint.target_point",
};

/** Draws the chip next to the pointer straight into the DOM: it moves at pointer speed (D2). */
function drawPreview(
  chip: HTMLElement | null,
  label: PreviewLabel | null,
  text: (label: PreviewLabel) => string,
  typedLabel: (label: PreviewLabel) => string,
) {
  if (chip === null) return;
  if (label === null) {
    chip.hidden = true;
    return;
  }
  chip.hidden = false;
  chip.style.transform = `translate(${label.at[0]}px, ${label.at[1]}px)`;
  chip.textContent = label.typed === null ? text(label) : typedLabel(label);
}

/**
 * The 3D view in the shell's viewport slot (VP-01): the canvas the WebAssembly viewport draws on,
 * with the tool bar, the view controls, the status, the hints and the states of boards 3.21–3.27.
 * Interaction happens in the viewport; this component sends commands and reacts to its events,
 * and turns its intents into commands of the API (UI-01).
 */
export function ViewportView() {
  const api = useApi();
  const { t, language } = useI18n();
  const { modalOpen, notify } = useShellState();
  const [selection, setSelection] = useState<readonly string[]>([]);
  const [tool, setTool] = useState<Tool>("select");
  const [step, setStep] = useState<ToolStep>("idle");
  const [projection, setProjection] = useState<Projection>("perspective");
  const [snapping, setSnapping] = useState(true);
  const [announcement, setAnnouncement] = useState({ text: "", serial: 0 });
  const [empty, setEmpty] = useState(false);
  const chip = useRef<HTMLDivElement>(null);
  const live = useRef({ t, language });
  live.current = { t, language };

  const chipText = useCallback(
    (label: PreviewLabel) => previewText(live.current.t, live.current.language, label),
    [],
  );
  const typedText = useCallback(
    (label: PreviewLabel) =>
      `${live.current.t(`viewport.typed.${label.key}`)} ${label.typed} ${live.current.t("viewport.typed.unit")} ↵`,
    [],
  );

  const onIntent = useRef<(intent: Intent) => void>(() => undefined);

  const onEvent = useCallback(
    (event: ViewportEvent) => {
      switch (event.kind) {
        case "selection":
          setSelection(event.ids);
          break;
        case "tool_state":
          setTool(event.tool);
          setStep(event.step);
          break;
        case "view_state":
          setProjection(event.state.projection);
          setSnapping(event.state.snapping);
          setTool(event.state.tool);
          break;
        case "announce":
          setAnnouncement((previous) => ({
            text: announcementText(live.current.t, live.current.language, event.key, event.values),
            serial: previous.serial + 1,
          }));
          break;
        case "preview_label":
          drawPreview(chip.current, event.label, chipText, typedText);
          break;
        case "intent":
          onIntent.current(event.intent);
          break;
        default:
          // Hover is drawn by the viewport and not announced (design/ux-notes.md).
          break;
      }
    },
    [chipText, typedText],
  );

  const session = useViewportSession(onEvent);
  const { phase, handle, feed, sceneLoaded, sceneVersion } = session;
  const ready = phase.kind === "ready";

  const command = useCallback((value: ViewCommand) => handle.current?.command(value), [handle]);

  const chooseTool = useCallback(
    (next: Tool) => {
      if (next === "move" && selection.length === 0) {
        notify(t("viewport.toast.moveNoSelection"));
        return;
      }
      handle.current?.setTool(next);
      setTool(next);
    },
    [handle, notify, selection.length, t],
  );

  onIntent.current = (intent) => {
    const metres = (value: number, digits: number) => formatMetres(language, value, digits);
    if (intent.kind === "move") {
      const count = intent.ids.length;
      const distance = metres(Math.hypot(...intent.translation), 3);
      moveElements(api, intent.ids, intent.translation).then(
        () =>
          notify(
            count === 1
              ? t("viewport.toast.moved.one", { distance })
              : t("viewport.toast.moved.other", { count, distance }),
            { undoable: true },
          ),
        (error: unknown) => {
          const reason = moveRefusal(error);
          notify(
            t(
              reason === "too_many"
                ? "viewport.toast.moveTooMany"
                : reason === null
                  ? "viewport.toast.moveFailed"
                  : "viewport.toast.moveRefused",
            ),
          );
        },
      );
      return;
    }
    const [width, depth, height] = intent.size;
    placeBlock(api, intent).then(
      async (id) => {
        notify(
          t("viewport.toast.placed", {
            width: metres(width, 2),
            depth: metres(depth, 2),
            height: metres(height, 2),
          }),
          { undoable: true },
        );
        // The block is selected once the view has it.
        await feed.current?.refresh();
        handle.current?.command({ kind: "select", ids: [id] });
      },
      () => notify(t("viewport.toast.placeFailed")),
    );
  };

  // What is drawn may have changed with each delta: the empty state follows the model.
  // biome-ignore lint/correctness/useExhaustiveDependencies: each new delta (`sceneVersion`) asks again.
  useEffect(() => {
    if (!sceneLoaded) return;
    let current = true;
    hasDrawableElements(api).then(
      (any) => {
        if (current) setEmpty(!any);
      },
      () => undefined,
    );
    return () => {
      current = false;
    };
  }, [api, sceneLoaded, sceneVersion]);

  // V, M, B, P and S, outside the canvas too; quiet behind a modal dialog and in text fields.
  useEffect(() => {
    if (!ready || modalOpen) return;
    const onKeyDown = (event: KeyboardEvent) => {
      const shortcut = viewportShortcut(event);
      if (shortcut === null) return;
      event.preventDefault();
      if (shortcut === "projection") {
        command({
          kind: "projection",
          projection: projection === "perspective" ? "parallel" : "perspective",
        });
      } else if (shortcut === "snapping") {
        command({ kind: "snapping", enabled: !snapping });
      } else {
        chooseTool(shortcut);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [ready, modalOpen, command, projection, snapping, chooseTool]);

  const hint = HINTS[step];
  const status = statusText(t, {
    phase: phase.kind,
    sceneLoaded,
    empty,
    count: selection.length,
    snapping,
  });

  return (
    <div className={`vp ${ready ? "" : "vp-inactive"}`}>
      {/* biome-ignore lint/a11y/noInteractiveElementToNoninteractiveRole: the focusable canvas is the interactive 3D view; "application" hands its keys to it. */}
      <canvas
        key={session.canvasKey}
        ref={session.canvasRef}
        className="vp-canvas"
        role="application"
        aria-label={t("viewport.canvas")}
        aria-describedby="vp-status"
        // Focusable, never a trap: Tab leaves it (the viewport does not take Tab).
        tabIndex={0}
      />
      <div className="vp-chip" ref={chip} hidden aria-hidden="true" />
      {ready && !sceneLoaded && <LoadingCard slow={false} onCancel={session.cancel} />}
      {phase.kind === "loading" && <LoadingCard slow={phase.slow} onCancel={session.cancel} />}
      {phase.kind === "unavailable" && <UnavailableCard />}
      {phase.kind === "failed" && (
        <FailedCard reason={phase.reason} repeated={phase.repeated} onReload={session.reload} />
      )}
      {ready && sceneLoaded && empty && step === "idle" && (
        <EmptyCard onPlaceBlock={() => chooseTool("place_block")} />
      )}
      {ready && sceneLoaded && !empty && <NavigationHint />}
      <div className="vp-bottom">
        {hint !== undefined && <p className="vp-hint">{t(hint)}</p>}
        <Toolbar tool={tool} disabled={!ready} onSelect={chooseTool} />
      </div>
      <p className="vp-status" id="vp-status">
        {status}
      </p>
      <ViewControls
        projection={projection}
        empty={empty}
        hasSelection={selection.length > 0}
        disabled={!ready}
        onCommand={command}
      />
      <div className="visually-hidden" role="status" aria-live="polite">
        <span key={announcement.serial}>{announcement.text}</span>
      </div>
    </div>
  );
}

type StatusInput = {
  phase: "loading" | "unavailable" | "ready" | "failed";
  sceneLoaded: boolean;
  empty: boolean;
  count: number;
  snapping: boolean;
};

/** The status bar text: the state of the view, else the selection and the snapping state. */
function statusText(
  t: (key: MessageKey, params?: Record<string, number>) => string,
  { phase, sceneLoaded, empty, count, snapping }: StatusInput,
): string {
  if (phase === "unavailable") return t("viewport.status.unavailable");
  if (phase === "failed") return t("viewport.status.stopped");
  if (phase === "loading" || !sceneLoaded) return t("viewport.status.loading");
  if (empty) return t("viewport.status.empty");
  const selected =
    count === 0
      ? t("viewport.status.none")
      : count === 1
        ? t("viewport.status.one")
        : t("viewport.status.other", { count });
  return `${selected} · ${t(snapping ? "viewport.status.snapOn" : "viewport.status.snapOff")}`;
}
