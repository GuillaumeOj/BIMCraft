import { useCallback, useEffect, useEffectEvent, useRef, useState } from "react";
import { type MoveRefusal, moveElements, moveRefusal, placeBlock, useApi } from "../api";
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

/** The toast for each reason a move did not happen; a refusal not listed reads as "can't move". */
const MOVE_FAILURES: Record<MoveRefusal | "failed", MessageKey> = {
  too_many: "viewport.toast.moveTooMany",
  out_of_range: "viewport.toast.moveOutOfRange",
  unknown_element: "viewport.toast.moveUnknown",
  not_movable: "viewport.toast.moveRefused",
  empty: "viewport.toast.moveRefused",
  failed: "viewport.toast.moveFailed",
};

/**
 * "N faces moved": which faces of the section box the user dragged since it was switched on or
 * last changed from the panel. A change of the box counts only while the viewport announces a
 * dragged face, so what Fit to selection, Reset or a typed extent does to the box is not counted.
 */
function useFacesMoved() {
  const moved = useRef(new Set<number>());
  const lastBox = useRef<SectionBox | null>(null);
  const dragging = useRef(false);
  const [facesMoved, setFacesMoved] = useState(0);
  const trackFaces = useCallback((box: SectionBox | null) => {
    const before = lastBox.current;
    lastBox.current = box;
    if (box === null || !box.enabled || before === null || !before.enabled) {
      moved.current.clear();
      dragging.current = false;
    } else if (dragging.current) {
      for (const face of changedFaces(before, box)) moved.current.add(face);
    }
    setFacesMoved(moved.current.size);
  }, []);
  /** The viewport announced a dragged face: the changes of the box that follow are drags. */
  const dragged = useCallback(() => {
    dragging.current = true;
  }, []);
  /** The panel changed the box: the count starts again, and what follows is not a drag. */
  const resetFaces = useCallback(() => {
    moved.current.clear();
    dragging.current = false;
    setFacesMoved(0);
  }, []);
  return { facesMoved, trackFaces, dragged, resetFaces };
}

/** The commands that change the section box other than by a drag of a face. */
const BOX_CHANGES = new Set<ViewCommand["kind"]>([
  "section_box_reset",
  "section_box_fit_selection",
  "section_box_extents",
]);

/** Whether the viewport draws no element at all. */
const isEmpty = (view: ViewState) => view.categories.every((category) => category.count === 0);

/** The hints a change of state calls for: they come once, when the state they explain begins. */
function stateHints(t: Translate, before: ViewState, next: ViewState): string[] {
  const hints: string[] = [];
  if (next.sectionBox?.enabled === true && before.sectionBox?.enabled !== true) {
    hints.push(t("viewport.hint.section"));
  }
  if (next.isolating && !before.isolating) {
    const names = isolatedLabel(next, t("viewport.hint.isolateSelection"));
    hints.push(t("viewport.hint.isolate", { names }));
  }
  return hints;
}

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

/** What Esc does at each step that shows a hint, when no value is being typed. */
const ESC_HINTS: Partial<Record<ToolStep, MessageKey>> = {
  first_corner: "viewport.hint.esc.cancel",
  base_point: "viewport.hint.esc.cancel",
  second_corner: "viewport.hint.esc.previous",
  height: "viewport.hint.esc.previous",
  target_point: "viewport.hint.esc.previous",
};

/**
 * The hint above the tool bar for `step`: what to do, then what Esc does, which is to clear the
 * value first while one is being typed.
 */
function stepHint(
  t: Translate,
  step: ToolStep,
  labelKey: PreviewLabel["key"] | null,
  typing: boolean,
): string | null {
  const hint =
    step === "second_corner" && labelKey === "place.depth" ? "viewport.hint.depth" : HINTS[step];
  if (hint === undefined) return null;
  const esc = typing ? "viewport.hint.esc.clear" : ESC_HINTS[step];
  return esc === undefined ? t(hint) : `${t(hint)} · ${t(esc)}`;
}

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
  const { facesMoved, trackFaces, dragged, resetFaces } = useFacesMoved();
  const [visibilityOpen, setVisibilityOpen] = useState(false);
  const [announcement, setAnnouncement] = useState({ text: "", serial: 0 });
  // Which value the pointer chip names: the hint bar follows it while the depth is typed.
  const [labelKey, setLabelKey] = useState<PreviewLabel["key"] | null>(null);
  // Whether a value is being typed: Esc then clears it before it steps back.
  const [typing, setTyping] = useState(false);
  const chip = useRef<HTMLDivElement>(null);
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
        if (event.key === "section_box.face_moved") dragged();
        setAnnouncement((previous) => ({
          text: announcementText(t, language, event.key, event.values),
          serial: previous.serial + 1,
        }));
        break;
      case "preview_label":
        drawPreview(chip.current, event.label, event.label === null ? "" : chipText(event.label));
        setLabelKey(event.label?.key ?? null);
        setTyping(event.label?.typed != null);
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
  const { phase, handle, feed, sceneLoaded, canvasRef } = session;
  // The viewport counts what it draws, by category: no element at all means an empty model.
  // Unknown (null) until the first scene: neither the empty card nor the hint shows.
  const empty = sceneLoaded ? isEmpty(view) : null;
  const ready = phase.kind === "ready";

  const command = useCallback(
    (value: ViewCommand) => {
      if (BOX_CHANGES.has(value.kind)) resetFaces();
      handle.current?.command(value);
    },
    [handle, resetFaces],
  );

  // Hints are toasts: they come once, when the state they explain begins.
  function onViewState(next: ViewState) {
    const before = lastView.current;
    lastView.current = next;
    setView(next);
    setTool(next.tool);
    trackFaces(next.sectionBox);
    if (before !== null) {
      for (const hint of stateHints(t, before, next)) notify(hint);
    }
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
          notify(t(MOVE_FAILURES[moveRefusal(error) ?? "failed"]));
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
  // viewport takes the key). Whatever else uses Esc (a popover, a field, a dialog) goes first,
  // so anything new that listens for Esc must call preventDefault, or it also ends the isolate.
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

  const hintText = stepHint(t, step, labelKey, typing);
  const status = statusText(t, {
    phase: phase.kind,
    sceneLoaded,
    empty: empty === true,
    count: selection.length,
    snapping,
    pickingFace: step === "pick_face",
    view,
    facesMoved,
    isolatedFallback: t("viewport.hint.isolateSelection"),
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
        {hintText !== null && <p className="vp-hint">{hintText}</p>}
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
  view: ViewState;
  facesMoved: number;
  /** What the status calls the selection when it is what is isolated. */
  isolatedFallback: string;
};

/** The names of the isolated groups, or `fallback` when the selection is isolated. */
function isolatedLabel(view: ViewState, fallback: string): string {
  const names = [...isolatedNames(view.categories), ...isolatedNames(view.disciplines)];
  return names.length > 0 ? names.join(", ") : fallback;
}

/** The status of what the user is doing to the view while nothing is selected, or null. */
function activityStatus(
  t: Translate,
  {
    view,
    facesMoved,
    isolatedFallback,
  }: Pick<StatusInput, "view" | "facesMoved" | "isolatedFallback">,
): string | null {
  const moved = view.sectionBox?.enabled === true ? facesMoved : 0;
  if (moved > 0) {
    return t(moved === 1 ? "viewport.status.section.one" : "viewport.status.section.other", {
      count: moved,
    });
  }
  if (view.isolating) {
    return t("viewport.status.isolated", { names: isolatedLabel(view, isolatedFallback) });
  }
  const planes = view.clipPlanes.length;
  if (planes === 0) return null;
  return t(planes === 1 ? "viewport.status.planes.one" : "viewport.status.planes.other", {
    count: planes,
  });
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
