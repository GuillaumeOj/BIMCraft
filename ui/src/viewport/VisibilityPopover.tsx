import { type KeyboardEvent, useEffect, useId, useRef, useState } from "react";
import { useI18n } from "../i18n/I18nProvider";
import { useShellState } from "../shell/ShellState";
import { shortcutProps } from "../ui/shortcut";
import { EyeIcon } from "./icons";
import type { ViewCommand, ViewState, VisibilityGroup, VisibilityTarget } from "./types";
import { isolatedNames } from "./viewState";

type Kind = "category" | "discipline";
const KINDS: readonly Kind[] = ["category", "discipline"];

/** The attribute that marks the button which opens the popover, so a press on it is not "outside". */
export const VISIBILITY_TRIGGER = "data-visibility-trigger";

type Props = {
  view: ViewState;
  onCommand: (command: ViewCommand) => void;
  onIsolateSelection: () => void;
  onHideSelection: () => void;
  hasSelection: boolean;
  /** Closes the popover; the opener takes the focus back. */
  onClose: () => void;
};

/**
 * The Visibility popover (board 3.2): the categories or the disciplines of the model with an eye
 * each. A roving-tabindex list, so every row is reachable from the keyboard: arrows and Home/End
 * move, Space hides or shows, Shift+Space isolates, Esc closes (board 3.26). It is not modal:
 * the view behind keeps working.
 */
export function VisibilityPopover({
  view,
  onCommand,
  onIsolateSelection,
  onHideSelection,
  hasSelection,
  onClose,
}: Props) {
  const { t } = useI18n();
  const { singleKeys } = useShellState();
  const ids = useId();
  const root = useRef<HTMLDivElement>(null);
  const [kind, setKind] = useState<Kind>("category");
  const [active, setActive] = useState(0);
  const groups = kind === "category" ? view.categories : view.disciplines;
  const rows = useRef<(HTMLButtonElement | null)[]>([]);
  const tabs = useRef<(HTMLButtonElement | null)[]>([]);

  // The first row takes the focus when the popover opens: the keys work at once.
  useEffect(() => {
    rows.current[0]?.focus();
  }, []);

  // A press outside closes it, as for the menus; the opener's own press is its click.
  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Element) || root.current?.contains(target)) return;
      if (target.closest(`[${VISIBILITY_TRIGGER}]`) === null) onClose();
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [onClose]);

  const target = (names: string[]): VisibilityTarget => ({ by: kind, names });
  const toggleHidden = (group: VisibilityGroup) =>
    onCommand({ kind: group.hidden ? "show" : "hide", target: target([group.name]) });
  const toggleIsolated = (group: VisibilityGroup) => {
    const current = isolatedNames(groups);
    const next = group.isolated
      ? current.filter((name) => name !== group.name)
      : [...current, group.name];
    onCommand(
      next.length === 0 ? { kind: "end_isolate" } : { kind: "isolate", target: target(next) },
    );
  };

  const focusRow = (index: number) => {
    const last = groups.length - 1;
    const next = Math.max(0, Math.min(index, last));
    setActive(next);
    rows.current[next]?.focus();
  };

  const onRowKeyDown = (
    event: KeyboardEvent<HTMLButtonElement>,
    group: VisibilityGroup,
    at: number,
  ) => {
    const last = groups.length - 1;
    const moves: Record<string, number> = {
      ArrowDown: at + 1,
      ArrowUp: at - 1,
      Home: 0,
      End: last,
    };
    const move = moves[event.key];
    if (move !== undefined) focusRow(move);
    // Space acts on the key down, so that Shift+Space is not also a click on the key up.
    else if (event.key === " ") act(event, group);
    else return;
    event.preventDefault();
  };
  const act = (event: KeyboardEvent, group: VisibilityGroup) => {
    if (event.repeat) return;
    if (event.shiftKey) toggleIsolated(group);
    else toggleHidden(group);
  };

  const selectKind = (next: Kind) => {
    setKind(next);
    setActive(0);
    tabs.current[KINDS.indexOf(next)]?.focus();
  };
  const onTabKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    const at = KINDS.indexOf(kind);
    const next =
      event.key === "ArrowRight"
        ? KINDS[(at + 1) % KINDS.length]
        : event.key === "ArrowLeft"
          ? KINDS[(at + KINDS.length - 1) % KINDS.length]
          : undefined;
    if (next === undefined) return;
    event.preventDefault();
    selectKind(next);
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    onClose();
  };

  const actionKey = (letter: string) => (singleKeys ? letter : undefined);
  const isolateLabel = t("viewport.visibility.isolate");
  const hideLabel = t("viewport.visibility.hide");

  return (
    <div
      className="vp-popover"
      role="dialog"
      aria-label={t("viewport.visibility.title")}
      ref={root}
      onKeyDown={onKeyDown}
    >
      <div className="vp-popover-head">
        <span className="vp-popover-title">{t("viewport.visibility.title")}</span>
        <div className="segmented" role="tablist" aria-label={t("viewport.visibility.group")}>
          {KINDS.map((value, index) => (
            <button
              key={value}
              type="button"
              role="tab"
              id={`${ids}-${value}`}
              aria-selected={value === kind}
              aria-controls={`${ids}-list`}
              tabIndex={value === kind ? 0 : -1}
              ref={(element) => {
                tabs.current[index] = element;
              }}
              onClick={() => selectKind(value)}
              onKeyDown={onTabKeyDown}
            >
              {t(`viewport.visibility.${value}`)}
            </button>
          ))}
        </div>
      </div>
      <div id={`${ids}-list`} role="tabpanel" aria-labelledby={`${ids}-${kind}`}>
        {groups.length === 0 ? (
          <p className="vp-popover-empty">{t(`viewport.visibility.none.${kind}`)}</p>
        ) : (
          <ul className="vp-rows">
            {groups.map((group, index) => (
              <li key={group.name}>
                <button
                  type="button"
                  role="switch"
                  aria-checked={!group.hidden}
                  className="vp-row"
                  data-isolated={group.isolated}
                  tabIndex={index === active ? 0 : -1}
                  ref={(element) => {
                    rows.current[index] = element;
                  }}
                  onFocus={() => setActive(index)}
                  onClick={(event) => (event.shiftKey ? toggleIsolated : toggleHidden)(group)}
                  onKeyDown={(event) => onRowKeyDown(event, group, index)}
                  onKeyUp={(event) => event.key === " " && event.preventDefault()}
                >
                  <EyeIcon hidden={group.hidden} />
                  <span className="vp-row-name">{group.name}</span>
                  {group.isolated && (
                    <span className="vp-tag">{t("viewport.visibility.isolated")}</span>
                  )}
                  {group.hidden && (
                    <span className="vp-tag">{t("viewport.visibility.hidden")}</span>
                  )}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
      <p className="vp-popover-keys">{t("viewport.visibility.keys")}</p>
      <div className="vp-popover-actions">
        <button
          type="button"
          className="button button-small"
          aria-disabled={!hasSelection}
          {...shortcutProps(t, isolateLabel, actionKey("I"))}
          onClick={onIsolateSelection}
        >
          {isolateLabel}
          {singleKeys && <kbd>{"I"}</kbd>}
        </button>
        <button
          type="button"
          className="button button-small"
          aria-disabled={!hasSelection}
          {...shortcutProps(t, hideLabel, actionKey("H"))}
          onClick={onHideSelection}
        >
          {hideLabel}
          {singleKeys && <kbd>{"H"}</kbd>}
        </button>
      </div>
    </div>
  );
}
