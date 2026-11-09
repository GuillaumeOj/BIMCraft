import { type ReactNode, useEffect, useId, useRef, useState } from "react";
import type { BuildUpPreview, MaterialSummary } from "../api";
import { formatPlain } from "../georef/units";
import { useI18n } from "../i18n/I18nProvider";
import type { MessageKey } from "../i18n/i18n";
import { ErrorIcon } from "../ui/ErrorIcon";
import { TextField } from "../ui/TextField";
import {
  canAddLayer,
  type Draft,
  type DraftLayer,
  insertLayer,
  moveLayer,
  newLayer,
  thicknessInMetres,
} from "./draft";
import type { EditorErrors } from "./errors";
import { LayerRow } from "./LayerRow";
import { SectionPreview } from "./SectionPreview";
import { Totals } from "./Totals";

type Props = {
  draft: Draft;
  onChange: (draft: Draft) => void;
  /** The project's materials, to choose from. */
  materials: readonly MaterialSummary[];
  /** The core's answer for the current draft; null until it has come. */
  preview: BuildUpPreview | null;
  errors: EditorErrors;
  /** Opens the Materials dialog for layer `key`. */
  onManageMaterials: (key: string) => void;
  /** An extra block above the table: where the wall-type editor adds the location line (ARC-30). */
  extra?: ReactNode;
  /** Called with live-region text for assistive technology. */
  announce: (message: string) => void;
};

/** What to focus once the table has re-rendered after a move, an addition or a removal. */
type Focus = { key: string; control: string };

/**
 * The reusable build-up editor: name, description, the layer table, the section preview and the
 * totals. It holds no state of its own beyond focus: the host owns the draft, asks the core for
 * the preview and applies (UX Part 2).
 */
export function BuildUpEditor(props: Props) {
  const { draft, onChange, errors } = props;
  const { t, language } = useI18n();
  const [selected, setSelected] = useState<number | null>(null);
  const focusAfter = useRef<Focus | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const [nameId, descriptionId] = [useId(), useId()];

  // The focused control of a moved row stays where the user is (same control of the same layer).
  useEffect(() => {
    const target = focusAfter.current;
    if (target === null) return;
    focusAfter.current = null;
    root.current
      ?.querySelector<HTMLElement>(
        `[data-layer="${CSS.escape(target.key)}"] [data-control="${target.control}"]`,
      )
      ?.focus();
  });

  const setLayers = (layers: DraftLayer[]) => onChange({ ...draft, layers });
  const patch = (index: number, change: Partial<DraftLayer>) =>
    setLayers(draft.layers.map((layer, at) => (at === index ? { ...layer, ...change } : layer)));

  const move = (index: number, to: number, control: string) => {
    const layer = draft.layers[index];
    if (layer === undefined) return;
    if (to < 0 || to >= draft.layers.length) {
      props.announce(t(to < 0 ? "buildups.layers.alreadyFirst" : "buildups.layers.alreadyLast"));
      return;
    }
    focusAfter.current = { key: layer.key, control };
    setLayers(moveLayer(draft.layers, index, to));
    setSelected(to);
    props.announce(
      t("buildups.layers.moved", {
        n: to + 1,
        total: draft.layers.length,
        material: props.materials.find((material) => material.id === layer.material)?.name ?? "",
        thickness: formatPlain(thicknessInMetres(layer.thickness) * 1000, 1, language),
      }),
    );
  };

  const remove = (index: number, control: string) => {
    if (draft.layers.length === 1) return;
    const layers = draft.layers.filter((_, at) => at !== index);
    // The same control of the layer that takes the place, or of the previous one at the end.
    const next = layers[index] ?? layers[index - 1];
    if (next !== undefined) focusAfter.current = { key: next.key, control };
    setLayers(layers);
    setSelected(null);
    props.announce(
      t(layers.length === 1 ? "buildups.layers.removedOne" : "buildups.layers.removedOther", {
        count: layers.length,
      }),
    );
  };

  const add = (after: number | null) => {
    if (!canAddLayer(draft)) return;
    const layer = newLayer();
    focusAfter.current = { key: layer.key, control: "function" };
    setLayers(insertLayer(draft.layers, after, layer));
    props.announce(
      t("buildups.layers.added", { n: (after === null ? draft.layers.length : after + 1) + 1 }),
    );
  };

  const [start, end] = [
    t(`buildups.layers.face.${draft.usage}.start` as MessageKey),
    t(`buildups.layers.face.${draft.usage}.end` as MessageKey),
  ];
  const last = draft.layers.length - 1;
  const resistances = props.preview?.layers ?? [];

  return (
    <div className="buildup-editor" ref={root}>
      <div className="editor-names">
        <TextField
          id={nameId}
          label={t("buildups.editor.name")}
          value={draft.name}
          onChange={(name) => onChange({ ...draft, name })}
          error={errors.name === null ? undefined : t(errors.name)}
          inputMode="text"
        />
        <TextField
          id={descriptionId}
          label={t("buildups.editor.description")}
          value={draft.description}
          onChange={(description) => onChange({ ...draft, description })}
          inputMode="text"
        />
      </div>
      <div className="editor-extra">{props.extra}</div>
      <h3 className="layers-title">{t("buildups.layers.title")}</h3>
      {errors.group.map((key) => (
        <p key={key} className="field-error" role="alert">
          <ErrorIcon />
          {t(key)}
        </p>
      ))}
      <table className="layer-table">
        <thead>
          <tr>
            <th scope="col">{t("buildups.layers.col.position")}</th>
            <th scope="col">{t("buildups.layers.col.function")}</th>
            <th scope="col">{t("buildups.layers.col.material")}</th>
            <th scope="col">{t("buildups.layers.col.thickness")}</th>
            <th scope="col">
              <HeaderWithHelp
                label={t("buildups.layers.col.priority")}
                title={t("buildups.priority.help.title")}
                body={`${t("buildups.priority.help")} ${t("buildups.priority.help.defaults")}`}
              />
            </th>
            <th scope="col">
              <HeaderWithHelp
                label={t("buildups.layers.col.core")}
                title={t("buildups.layers.col.core")}
                body={t("buildups.core.help")}
              />
            </th>
            <th scope="col">{t("buildups.layers.col.resistance")}</th>
            <th scope="col">
              <span className="visually-hidden">{t("buildups.layers.col.actions")}</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {draft.layers.map((layer, index) => (
            <LayerRow
              key={layer.key}
              index={index}
              count={draft.layers.length}
              layer={layer}
              face={index === 0 ? start : index === last ? end : null}
              materials={props.materials}
              errors={errors.rows[index] ?? { inCoreSpan: false }}
              resistance={resistances[index]?.thermal_resistance}
              selected={selected === index}
              onSelect={() => setSelected(index)}
              onChange={(change) => patch(index, change)}
              onManage={() => props.onManageMaterials(layer.key)}
              onMove={(to) => move(index, to, focusedControl(root.current))}
              onRemove={() => remove(index, "remove")}
              onAdd={() => add(index)}
            />
          ))}
        </tbody>
      </table>
      <div className="add-layer">
        <button
          type="button"
          className="button"
          aria-disabled={!canAddLayer(draft)}
          title={canAddLayer(draft) ? undefined : t("buildups.layers.max")}
          onClick={() => add(selected)}
        >
          {t("buildups.layers.add")}
        </button>
        <span className="hint">{t("buildups.layers.addHint")}</span>
      </div>
      <div className="editor-summary">
        <SectionPreview draft={draft} selected={selected} />
        <Totals preview={props.preview} />
      </div>
    </div>
  );
}

/** The `data-control` of the focused control of the table, so that a move keeps focus on it. */
function focusedControl(root: HTMLElement | null): string {
  const active = document.activeElement;
  return root?.contains(active) && active instanceof HTMLElement
    ? (active.getAttribute("data-control") ?? "up")
    : "up";
}

/** A column header with an info button that shows `body` under it. */
function HeaderWithHelp({ label, title, body }: { label: string; title: string; body: string }) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const id = useId();
  return (
    <span className="header-help">
      <span>{label}</span>
      <button
        type="button"
        className="info-button"
        aria-expanded={open}
        aria-controls={id}
        aria-label={t("buildups.help.button", { label: title })}
        onClick={() => setOpen((value) => !value)}
        onKeyDown={(event) => {
          if (event.key === "Escape" && open) {
            event.preventDefault();
            setOpen(false);
          }
        }}
      >
        <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
          <circle cx="8" cy="8" r="6.5" fill="none" stroke="currentColor" />
          <path d="M8 7v4M8 5v.01" stroke="currentColor" strokeWidth="1.5" />
        </svg>
      </button>
      <span id={id} className="help-popover" role="note" hidden={!open}>
        <strong>{title}</strong>
        {body}
      </span>
    </span>
  );
}
