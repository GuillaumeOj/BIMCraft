import { useCallback, useEffect, useEffectEvent, useRef, useState } from "react";
import { hasDrawableElements, moveElements, moveRefusal, placeBlock, useApi } from "../api";
import { useI18n } from "../i18n/I18nProvider";
import type { MessageKey } from "../i18n/i18n";
import { useShellState } from "../shell/ShellState";
import { announcementText, formatMetres, previewText } from "./format";
import { NavigationHint } from "./HintCard";
import { EmptyCard, FailedCard, LoadingCard, UnavailableCard } from "./StateCards";
import { isTextEntry, type ViewportShortcut, viewportShortcut } from "./shortcuts";
import { Toolbar } from "./Toolbar";
import type {
  Intent,
  PreviewLabel,
  Tool,
  ToolStep,
  ViewCommand,
  ViewportEvent,
  ViewState,
} from "./types";
import { type Phase, useViewportSession } from "./useViewportSession";
import { ViewControls } from "./ViewControls";
import { IDLE_REPORT, usePublishViewReport } from "./ViewportBridge";
import { VISIBILITY_TRIGGER, VisibilityPopover } from "./VisibilityPopover";
import { changedFaces, isolatedNames, type SectionBox } from "./viewState";
import "./viewport.css";

/** The steps that show a hint above the tool bar. */
const HINTS: Partial<Record<ToolStep, MessageKey>> = {
  first_corner: "viewport.hint.first_corner",
  second_corner: "viewport.hint.second_corner",
  height: "viewport.hint.height",
  base_point: "viewport.hint.base_point",
  target_point: "viewport.hint.target_point",
  pick_face: "viewport.hint.pick_face",
};

/** What the viewport reports until its first change: nothing hidden, no cut, perspective. */
const INITIAL_VIEW: ViewState = {
  projection: "perspective",
  snapping: true,
  tool: "select",
  categories: [],
  disciplines: [],
  isolating: false,
  hiddenSelection: 0,
  sectionBox: null,
  clipPlanes: [],
  cutFaces: "tinted",
};

/** The commands that give the section box a new starting point: moves are counted from it. */
const REBASES = new Set<ViewCommand["kind"]>([
  "section_box",
  "section_box_reset",
  "section_box_fit_selection",
]);

/** Pointers lower than this above the bottom edge get their chip above them: the band of the tool bar and the hint. */
const BOTTOM_BAND = 150;
/** How far above the pointer a chip goes when it cannot go below. */
const FLIP = 48;

/**
 * Draws the chip next to the pointer straight into the DOM: it moves at pointer speed (D2). A
 * typed value looks like a field, so users see where their typing goes (board 3.28).
 */
function drawPreview(chip: HTMLElement | null, label: PreviewLabel | null, text: string) {
  if (chip === null) return;
  if (label === null) {
    chip.hidden = true;
    return;
  }
  const [x, y] = label.at;
  const height = chip.parentElement?.clientHeight ?? 0;
  const top = height > 0 && y > height - BOTTOM_BAND ? y - FLIP : y;
  chip.hidden = false;
  chip.style.transform = `translate(${x}px, ${top}px)`;
  chip.classList.toggle("vp-chip-typed", label.typed !== null);
  chip.textContent = text;
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
  const { modalOpen, notify, singleKeys } = useShellState();
  const [selection, setSelection] = useState<readonly string[]>([]);
  const [tool, setTool] = useState<Tool>("select");
  const [step, setStep] = useState<ToolStep>("idle");
  const [view, setView] = useState(INITIAL_VIEW);
  const [facesMoved, setFacesMoved] = useState(0);
  const [visibilityOpen, setVisibilityOpen] = useState(false);
  const [announcement, setAnnouncement] = useState({ text: "", serial: 0 });
  // Unknown (null) until the model was first asked: neither the empty card nor the hint shows.
  const [empty, setEmpty] = useState<boolean | null>(null);
  // Which value the pointer chip names: the hint bar follows it while the depth is typed.
  const [labelKey, setLabelKey] = useState<PreviewLabel["key"] | null>(null);
  const chip = useRef<HTMLDivElement>(null);
  // Which faces of the section box moved since it last got a starting point, and the box before.
  const moved = useRef(new Set<number>());
  const lastBox = useRef<SectionBox | null>(null);
  const rebase = useRef(false);
  const lastView = useRef<ViewState | null>(null);
  const { projection, snapping } = view;

  const chipText = (label: PreviewLabel) =>
    label.typed === null
      ? previewText(t, language, label)
      : `${t(`viewport.typed.${label.key}`)} ${label.typed} ${t("viewport.typed.unit")} ↵`;

  // The session always calls the latest version of this function: it reads this render's state.
  const onEvent = (event: ViewportEvent) => {
    switch (event.kind) {
      case "selection":
        setSelection(event.ids);
        break;
      case "tool_state":
        setTool(event.tool);
        setStep(event.step);
        break;
      case "view_state":
        onViewState(event.state);
        break;
      case "announce":
        setAnnouncement((previous) => ({
          text: announcementText(t, language, event.key, event.values),
          serial: previous.serial + 1,
        }));
        break;
      case "preview_label":
        drawPreview(chip.current, event.label, event.label === null ? "" : chipText(event.label));
        setLabelKey(event.label?.key ?? null);
        break;
      case "intent":
        runIntent(event.intent);
        break;
      default:
        // Hover is drawn by the viewport and not announced (design/ux-notes.md).
        break;
    }
  };

  const session = useViewportSession(onEvent);
  const { phase, handle, feed, sceneLoaded, sceneVersion, canvasRef } = session;
  const ready = phase.kind === "ready";

  const command = useCallback(
    (value: ViewCommand) => {
      if (REBASES.has(value.kind)) rebase.current = true;
      handle.current?.command(value);
    },
    [handle],
  );

  // Hints are toasts: they come once, when the state they explain begins.
  function onViewState(next: ViewState) {
    const before = lastView.current;
    lastView.current = next;
    setView(next);
    setTool(next.tool);
    trackFaces(next.sectionBox);
    if (before === null) return;
    if (next.sectionBox?.enabled === true && before.sectionBox?.enabled !== true) {
      notify(t("viewport.hint.section"));
    }
    if (next.isolating && !before.isolating) {
      const names = [...isolatedNames(next.categories), ...isolatedNames(next.disciplines)];
      notify(
        t("viewport.hint.isolate", {
          names: names.length > 0 ? names.join(", ") : t("viewport.hint.isolateSelection"),
        }),
      );
    }
  }

  // "N faces moved": the faces whose position changed since the box was switched on or reset.
  function trackFaces(box: SectionBox | null) {
    const before = lastBox.current;
    lastBox.current = box;
    if (box === null || !box.enabled || before === null || !before.enabled || rebase.current) {
      moved.current.clear();
    } else {
      for (const face of changedFaces(before, box)) moved.current.add(face);
    }
    if (box?.enabled === true) rebase.current = false;
    setFacesMoved(moved.current.size);
  }

  const chooseTool = (next: Tool) => {
    if (next === "move" && selection.length === 0) {
      notify(t("viewport.toast.moveNoSelection"));
      return;
    }
    handle.current?.setTool(next);
    setTool(next);
  };

  function runIntent(intent: Intent) {
    const metres = (value: number) => formatMetres(language, value);
    if (intent.kind === "move") {
      const count = intent.ids.length;
      const distance = metres(Math.hypot(...intent.translation));
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
            width: metres(width),
            depth: metres(depth),
            height: metres(height),
          }),
          { undoable: true },
        );
        // The block is selected once the view has it.
        await feed.current?.refresh();
        handle.current?.command({ kind: "select", ids: [id] });
      },
      () => notify(t("viewport.toast.placeFailed")),
    );
  }

  // What is drawn may have changed with each delta: the empty state follows the model.
  useEffect(() => {
    // No delta yet: nothing to ask about.
    if (sceneVersion === 0) return;
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
  }, [api, sceneVersion]);

  const hideOrIsolate = (kind: "hide" | "isolate") => {
    if (selection.length === 0) {
      notify(t("viewport.toast.noSelectionVisibility"));
      return;
    }
    command({ kind, target: { by: "selection" } });
  };

  // Reads this render's state; the listener below stays subscribed across renders.
  const onShortcut = useEffectEvent((shortcut: ViewportShortcut) => {
    switch (shortcut) {
      case "projection":
        command({
          kind: "projection",
          projection: projection === "perspective" ? "parallel" : "perspective",
        });
        break;
      case "snapping":
        command({ kind: "snapping", enabled: !snapping });
        break;
      case "hide":
      case "isolate":
        hideOrIsolate(shortcut);
        break;
      case "reset_visibility":
        command({ kind: "reset_visibility" });
        break;
      case "section_box":
        command({ kind: "section_box", enabled: view.sectionBox?.enabled !== true });
        break;
      default:
        chooseTool(shortcut);
    }
  });

  // Esc ends a pending face pick, then an isolate, from outside the canvas too (inside it, the
  // viewport takes the key). Whatever else uses Esc (a popover, a field, a dialog) goes first.
  const cancellable = view.isolating || step === "pick_face";
  const onEscape = useEffectEvent((event: KeyboardEvent) => {
    if (event.key !== "Escape" || event.defaultPrevented || event.target === canvasRef.current) {
      return;
    }
    if (event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return;
    if (event.target instanceof HTMLElement && isTextEntry(event.target)) return;
    command({ kind: "cancel" });
  });

  // V, M, B, P, S, H, I and Alt+H, Alt+B, outside the canvas too; quiet behind a modal dialog and in text fields.
  useEffect(() => {
    if (!ready || modalOpen) return;
    const onKeyDown = (event: KeyboardEvent) => {
      const shortcut = viewportShortcut(event, { singleKeys, canvas: canvasRef.current });
      if (shortcut === null) return;
      event.preventDefault();
      onShortcut(shortcut);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [ready, modalOpen, singleKeys, canvasRef]);

  useEffect(() => {
    if (!ready || modalOpen || !cancellable) return;
    window.addEventListener("keydown", onEscape);
    return () => window.removeEventListener("keydown", onEscape);
  }, [ready, modalOpen, cancellable]);

  // The View panel sits beside the view: it renders this report and sends commands through it.
  const publish = usePublishViewReport();
  const selectionCount = selection.length;
  useEffect(() => {
    publish(ready ? { view, step, selectionCount, facesMoved, send: command } : IDLE_REPORT);
    return () => publish(IDLE_REPORT);
  }, [publish, ready, view, step, selectionCount, facesMoved, command]);

  // Esc and a press outside close the popover; the focus goes back to the button that opened it.
  const closeVisibility = useCallback(() => {
    setVisibilityOpen(false);
    document.querySelector<HTMLElement>(`[${VISIBILITY_TRIGGER}]`)?.focus();
  }, []);

  const hint =
    step === "second_corner" && labelKey === "place.depth" ? "viewport.hint.depth" : HINTS[step];
  const status = statusText(t, {
    phase: phase.kind,
    sceneLoaded,
    empty: empty === true,
    count: selection.length,
    snapping,
    pickingFace: step === "pick_face",
    facesMoved: view.sectionBox?.enabled === true ? facesMoved : 0,
    isolated: view.isolating ? isolatedLabel(view, t("viewport.hint.isolateSelection")) : null,
    planes: view.clipPlanes.length,
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
      {(phase.kind === "loading" || (ready && !sceneLoaded)) && (
        <LoadingCard slow={session.slow} onCancel={session.cancel} />
      )}
      {phase.kind === "unavailable" && <UnavailableCard />}
      {phase.kind === "failed" && (
        <FailedCard reason={phase.reason} repeated={phase.repeated} onReload={session.reload} />
      )}
      {ready && sceneLoaded && empty === true && step === "idle" && (
        <EmptyCard onPlaceBlock={() => chooseTool("place_block")} />
      )}
      {ready && sceneLoaded && empty === false && <NavigationHint />}
      <div className="vp-bottom">
        {hint !== undefined && <p className="vp-hint">{t(hint)}</p>}
        <Toolbar tool={tool} disabled={!ready} onSelect={chooseTool} />
      </div>
      <p className="vp-status" id="vp-status">
        {status}
      </p>
      {visibilityOpen && ready && (
        <VisibilityPopover
          view={view}
          hasSelection={selection.length > 0}
          onCommand={command}
          onIsolateSelection={() => hideOrIsolate("isolate")}
          onHideSelection={() => hideOrIsolate("hide")}
          onClose={closeVisibility}
        />
      )}
      <ViewControls
        projection={projection}
        empty={empty !== false}
        hasSelection={selection.length > 0}
        disabled={!ready}
        visibilityOpen={visibilityOpen}
        onVisibility={() => setVisibilityOpen((open) => !open)}
        onCommand={command}
      />
      <div className="visually-hidden" role="status" aria-live="polite">
        <span key={announcement.serial}>{announcement.text}</span>
      </div>
    </div>
  );
}

type Translate = (key: MessageKey, params?: Record<string, number | string>) => string;

type StatusInput = {
  phase: Phase["kind"];
  sceneLoaded: boolean;
  empty: boolean;
  count: number;
  snapping: boolean;
  pickingFace: boolean;
  facesMoved: number;
  /** The names of what is isolated, or null when nothing is. */
  isolated: string | null;
  planes: number;
};

/** The names of the isolated groups, or `fallback` when the selection is isolated. */
function isolatedLabel(view: ViewState, fallback: string): string {
  const names = [...isolatedNames(view.categories), ...isolatedNames(view.disciplines)];
  return names.length > 0 ? names.join(", ") : fallback;
}

/** The status of what the user is doing to the view while nothing is selected, or null. */
function activityStatus(
  t: Translate,
  { facesMoved, isolated, planes }: Pick<StatusInput, "facesMoved" | "isolated" | "planes">,
): string | null {
  if (facesMoved > 0) {
    return t(facesMoved === 1 ? "viewport.status.section.one" : "viewport.status.section.other", {
      count: facesMoved,
    });
  }
  if (isolated !== null) return t("viewport.status.isolated", { names: isolated });
  if (planes > 0) {
    return t(planes === 1 ? "viewport.status.planes.one" : "viewport.status.planes.other", {
      count: planes,
    });
  }
  return null;
}

/**
 * The status bar text: the state of the view, then what is being done to it (picking a face,
 * moving the section box, isolating, clipping) while nothing is selected, else the selection and
 * the snapping state.
 */
function statusText(t: Translate, input: StatusInput): string {
  const { phase, sceneLoaded, empty, count, snapping } = input;
  if (phase === "unavailable") return t("viewport.status.unavailable");
  if (phase === "failed") return t("viewport.status.stopped");
  if (phase === "loading" || !sceneLoaded) return t("viewport.status.loading");
  if (empty) return t("viewport.status.empty");
  if (input.pickingFace) return t("viewport.status.pickFace");
  const activity = count === 0 ? activityStatus(t, input) : null;
  if (activity !== null) return activity;
  const selected =
    count === 0
      ? t("viewport.status.none")
      : count === 1
        ? t("viewport.status.one")
        : t("viewport.status.other", { count });
  return `${selected} · ${t(snapping ? "viewport.status.snapOn" : "viewport.status.snapOff")}`;
}
