import { useI18n } from "../i18n/I18nProvider";
import { ActionMenu } from "../ui/ActionMenu";
import { SegmentedToggle } from "../ui/SegmentedToggle";
import { PresetsIcon, ZoomFitIcon, ZoomSelectionIcon } from "./icons";
import type { Preset, Projection, ViewCommand } from "./types";

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
  onCommand: (command: ViewCommand) => void;
};

/**
 * The view-controls cluster, bottom right (board 3.21): zoom fit, zoom to selection, the preset
 * views menu and the projection toggle. Each button names its shortcut.
 */
export function ViewControls({ projection, empty, hasSelection, disabled, onCommand }: Props) {
  const { t } = useI18n();
  const zoomFit = t("viewport.zoomFit");
  const zoomSelection = t("viewport.zoomSelection");
  return (
    <div className="vp-controls" role="toolbar" aria-label={t("viewport.controls")}>
      <button
        type="button"
        className="vp-icon-button"
        aria-label={zoomFit}
        aria-keyshortcuts="F"
        title={t("viewport.withShortcut", { label: zoomFit, shortcut: "F" })}
        aria-disabled={disabled || empty}
        onClick={() => {
          if (!(disabled || empty)) onCommand({ kind: "zoom_fit" });
        }}
      >
        <ZoomFitIcon />
      </button>
      <button
        type="button"
        className="vp-icon-button"
        aria-label={zoomSelection}
        aria-keyshortcuts="Shift+F"
        title={t("viewport.withShortcut", { label: zoomSelection, shortcut: "Shift+F" })}
        aria-disabled={disabled || !hasSelection}
        onClick={() => {
          if (!disabled && hasSelection) onCommand({ kind: "zoom_selection" });
        }}
      >
        <ZoomSelectionIcon />
      </button>
      <ActionMenu
        className="vp-icon-button"
        label={t("viewport.presets")}
        items={PRESETS.map((preset) => ({
          label: t(`viewport.preset.${preset}`),
          onSelect: () => onCommand({ kind: "view_preset", preset }),
        }))}
      >
        <PresetsIcon />
      </ActionMenu>
      <SegmentedToggle<Projection>
        label={t("viewport.projection")}
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
