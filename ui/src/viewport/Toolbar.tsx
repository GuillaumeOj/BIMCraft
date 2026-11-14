import type { ReactNode } from "react";
import { useI18n } from "../i18n/I18nProvider";
import { useShellState } from "../shell/ShellState";
import { shortcutProps } from "../ui/shortcut";
import { BlockIcon, MoveIcon, SelectIcon } from "./icons";
import type { Tool } from "./types";

/** The tools the tool bar offers, with their global shortcut (design/ux-notes.md). */
export const TOOLS: readonly { tool: Tool; shortcut: string; icon: () => ReactNode }[] = [
  { tool: "select", shortcut: "V", icon: SelectIcon },
  { tool: "move", shortcut: "M", icon: MoveIcon },
  { tool: "place_block", shortcut: "B", icon: BlockIcon },
];

type Props = {
  tool: Tool;
  disabled: boolean;
  onSelect: (tool: Tool) => void;
};

/** The bottom tool bar: Select, Move and Block (board 3.21), pressed for the active tool. */
export function Toolbar({ tool, disabled, onSelect }: Props) {
  const { t } = useI18n();
  // The keys work outside the view only while single-key shortcuts are on: only then are they named.
  const { singleKeys } = useShellState();
  return (
    <div className="vp-toolbar" role="toolbar" aria-label={t("viewport.tools")}>
      {TOOLS.map(({ tool: value, shortcut, icon: ToolIcon }) => {
        const label = t(`viewport.tool.${value}`);
        return (
          <button
            key={value}
            type="button"
            className="vp-tool"
            aria-pressed={value === tool}
            {...shortcutProps(t, label, singleKeys ? shortcut : undefined)}
            disabled={disabled}
            onClick={() => onSelect(value)}
          >
            <ToolIcon />
            <span>{label}</span>
          </button>
        );
      })}
    </div>
  );
}
