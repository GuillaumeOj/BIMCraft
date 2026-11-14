import type { ReactNode } from "react";
import { useI18n } from "../i18n/I18nProvider";
import { useShellState } from "../shell/ShellState";
import { ActionMenu } from "../ui/ActionMenu";
import { SegmentedToggle } from "../ui/SegmentedToggle";
import { shortcutProps } from "../ui/shortcut";
import { EyeIcon, PresetsIcon, ZoomFitIcon, ZoomSelectionIcon } from "./icons";
import type { Preset, Projection, ViewCommand } from "./types";
import { VISIBILITY_TRIGGER } from "./VisibilityPopover";

const PRESETS: readonly Preset[] = [
  "top",
  "front",
  "back",
  "left",
  "right",
  "iso_se",
  "iso_sw",
  "iso_ne",
  "iso_nw",
];

type Props = {
  projection: Projection;
  /** Nothing to frame: zoom fit is off. */
  empty: boolean;
  hasSelection: boolean;
  disabled: boolean;
  /** Whether the Visibility popover is open. */
  visibilityOpen: boolean;
  onVisibility: () => void;
  onCommand: (command: ViewCommand) => void;
};

/**
 * The view-controls cluster, bottom right (board 3.21): zoom fit, zoom to selection, the preset
 * views menu and the projection toggle. Each button names its shortcut.
 */
export function ViewControls({
  projection,
  empty,
  hasSelection,
  disabled,
  visibilityOpen,
  onVisibility,
  onCommand,
}: Props) {
  const { t } = useI18n();
  const { singleKeys } = useShellState();
  return (
    <div className="vp-controls" role="toolbar" aria-label={t("viewport.controls")}>
      {/* F and Shift+F belong to the focused view, so they are always named. */}
      <CommandButton
        label={t("viewport.zoomFit")}
        shortcut="F"
        off={disabled || empty}
        onClick={() => onCommand({ kind: "zoom_fit" })}
      >
        <ZoomFitIcon />
      </CommandButton>
      <CommandButton
        label={t("viewport.zoomSelection")}
        shortcut="Shift+F"
        off={disabled || !hasSelection}
        onClick={() => onCommand({ kind: "zoom_selection" })}
      >
        <ZoomSelectionIcon />
      </CommandButton>
      <ActionMenu
        className="vp-icon-button"
        label={t("viewport.presets")}
        placement="up"
        items={PRESETS.map((preset) => ({
          label: t(`viewport.preset.${preset}`),
          onSelect: () => onCommand({ kind: "view_preset", preset }),
        }))}
      >
        <PresetsIcon />
      </ActionMenu>
      <button
        type="button"
        className="vp-icon-button"
        {...{ [VISIBILITY_TRIGGER]: "" }}
        aria-label={t("viewport.visibility.title")}
        title={t("viewport.visibility.title")}
        aria-haspopup="dialog"
        aria-expanded={visibilityOpen}
        disabled={disabled}
        onClick={onVisibility}
      >
        <EyeIcon hidden={false} />
      </button>
      <SegmentedToggle<Projection>
        label={t("viewport.projection")}
        shortcut={singleKeys ? "P" : undefined}
        value={projection}
        options={[
          { value: "perspective", label: t("viewport.projection.perspective") },
          { value: "parallel", label: t("viewport.projection.parallel") },
        ]}
        onChange={(value) => onCommand({ kind: "projection", projection: value })}
      />
    </div>
  );
}

type CommandButtonProps = {
  label: string;
  shortcut: string;
  /** Unavailable: announced as disabled, and a click does nothing. */
  off: boolean;
  onClick: () => void;
  children: ReactNode;
};

/** An icon button that stays focusable while unavailable (`aria-disabled`, rule 4). */
function CommandButton({ label, shortcut, off, onClick, children }: CommandButtonProps) {
  const { t } = useI18n();
  return (
    <button
      type="button"
      className="vp-icon-button"
      aria-label={label}
      {...shortcutProps(t, label, shortcut)}
      aria-disabled={off}
      onClick={() => {
        if (!off) onClick();
      }}
    >
      {children}
    </button>
  );
}
