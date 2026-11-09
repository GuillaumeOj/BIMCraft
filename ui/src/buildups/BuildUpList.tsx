import { type KeyboardEvent, type RefObject, useEffect, useId, useRef, useState } from "react";
import type { BuildUpSummary, BuildUpUsage } from "../api";
import { buildUpRefusal } from "../api";
import { formatPlain } from "../georef/units";
import { useI18n } from "../i18n/I18nProvider";
import type { MessageKey } from "../i18n/i18n";
import { MORE_DOTS } from "../materials/glyphs";
import { ActionMenu } from "../ui/ActionMenu";
import { Banner } from "../ui/Banner";
import { SegmentedToggle } from "../ui/SegmentedToggle";
import { TextField } from "../ui/TextField";
import { failureMessage, pluralKey } from "./messages";

export type Filter = "all" | BuildUpUsage;

/** What the list asks its host to do; a rejected promise is a refusal to show at the field. */
export type ListActions = {
  onEdit: (id: string) => void;
  onNew: (usage: BuildUpUsage) => void;
  onDuplicate: (id: string, name: string) => Promise<void>;
  onRename: (id: string, name: string) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
};

type Props = ListActions & {
  items: readonly BuildUpSummary[];
  /** True until the first answer; the list shows its loading line. */
  loading: boolean;
  loadFailed: boolean;
  onRetry: () => void;
  filter: Filter;
  onFilter: (filter: Filter) => void;
  selected: string | null;
  onSelect: (id: string) => void;
  /** The list's own focus target, so that the host can return focus to it. */
  listRef: RefObject<HTMLElement | null>;
};

type Inline = { kind: "duplicate" | "rename" | "delete"; id: string } | null;

/** The build-up list: filter, new menu, rows with their menu, and the inline forms (F2-A…E). */
export function BuildUpList(props: Props) {
  const { t, language } = useI18n();
  const listId = useId();
  const [inline, setInline] = useState<Inline>(null);
  // Closing an inline form or confirmation returns focus to its row (UX focus order).
  const closeInline = (id: string) => {
    setInline(null);
    // After the render that puts the row back.
    setTimeout(() => document.getElementById(`${listId}-${id}`)?.focus());
  };
  const { items, selected } = props;
  const at = items.findIndex((item) => item.id === selected);
  const empty = !props.loading && !props.loadFailed && items.length === 0;

  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (
      inline !== null ||
      !(event.target instanceof HTMLElement) ||
      !event.target.matches(".row-main")
    )
      return;
    const target = { ArrowDown: at + 1, ArrowUp: at - 1, Home: 0, End: items.length - 1 }[
      event.key as "ArrowDown" | "ArrowUp" | "Home" | "End"
    ];
    const item = target === undefined ? undefined : items[target];
    if (item !== undefined) {
      event.preventDefault();
      props.onSelect(item.id);
      setTimeout(() => document.getElementById(`${listId}-${item.id}`)?.focus());
    } else if (selected !== null && event.key === "Delete") {
      event.preventDefault();
      setInline({ kind: "delete", id: selected });
    } else if (selected !== null && event.key === "F2") {
      event.preventDefault();
      setInline({ kind: "rename", id: selected });
    }
  };
  return (
    <div className="buildups-list">
      <div className="list-toolbar">
        <SegmentedToggle
          label={t("buildups.filter.label")}
          value={props.filter}
          options={[
            { value: "all", label: t("buildups.filter.all") },
            { value: "wall", label: t("buildups.filter.wall") },
            { value: "slab", label: t("buildups.filter.slab") },
          ]}
          onChange={props.onFilter}
        />
        <ActionMenu
          className="button button-primary"
          items={[
            { label: t("buildups.action.newWall"), onSelect: () => props.onNew("wall") },
            { label: t("buildups.action.newSlab"), onSelect: () => props.onNew("slab") },
          ]}
        >
          {t("buildups.action.new")}
        </ActionMenu>
      </div>
      {props.loading && <p className="empty-note">{t("buildups.loading")}</p>}
      {props.loadFailed && (
        <div>
          <Banner>{t("buildups.loadError")}</Banner>
          <button type="button" className="button" onClick={props.onRetry}>
            {t("buildups.retry")}
          </button>
        </div>
      )}
      {empty && <EmptyState filter={props.filter} onNew={props.onNew} />}
      <section
        ref={props.listRef}
        className="buildup-rows"
        aria-label={t("buildups.list.label")}
        onKeyDown={onKeyDown}
      >
        {items.map((item) => (
          <div key={item.id} className="buildup-row" data-selected={item.id === selected}>
            {inline?.id === item.id && inline.kind === "rename" ? (
              <NameForm
                label={t("buildups.rename.label")}
                initial={item.name}
                submit={async (name) =>
                  name === item.name ? undefined : props.onRename(item.id, name)
                }
                onDone={() => closeInline(item.id)}
              />
            ) : (
              <>
                <button
                  type="button"
                  id={`${listId}-${item.id}`}
                  className="row-main"
                  aria-current={item.id === selected}
                  tabIndex={
                    item.id === selected || (selected === null && item === items[0]) ? 0 : -1
                  }
                  onClick={() => props.onSelect(item.id)}
                  onDoubleClick={() => props.onEdit(item.id)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      event.preventDefault();
                      props.onEdit(item.id);
                    }
                  }}
                >
                  <span className="row-name">{item.name}</span>
                  <span className="row-meta">
                    {` ${t(pluralKey(language, item.layer_count, "buildups.row.summary"), {
                      count: item.layer_count,
                      total: formatPlain(item.total_thickness * 1000, 1, language),
                    })}`}
                  </span>
                </button>
                <span className="tag">{t(`buildups.usage.${item.usage}` as MessageKey)}</span>
                <ActionMenu
                  className="icon-button"
                  label={t("buildups.menu.label", { name: item.name })}
                  items={[
                    { label: t("buildups.action.edit"), onSelect: () => props.onEdit(item.id) },
                    {
                      label: t("buildups.action.duplicate"),
                      onSelect: () => setInline({ kind: "duplicate", id: item.id }),
                    },
                    {
                      label: t("buildups.action.rename"),
                      onSelect: () => setInline({ kind: "rename", id: item.id }),
                    },
                    {
                      label: t("buildups.action.delete"),
                      onSelect: () => setInline({ kind: "delete", id: item.id }),
                    },
                  ]}
                >
                  <span aria-hidden="true">{MORE_DOTS}</span>
                </ActionMenu>
              </>
            )}
            {inline?.id === item.id && inline.kind === "duplicate" && (
              <NameForm
                label={t("buildups.duplicate.name")}
                initial={t("buildups.duplicate.suggest", { name: item.name })}
                submit={(name) => props.onDuplicate(item.id, name)}
                onDone={() => closeInline(item.id)}
                confirmLabel={t("buildups.duplicate.confirm")}
              />
            )}
            {inline?.id === item.id && inline.kind === "delete" && (
              <DeleteConfirmation
                item={item}
                onKeep={() => closeInline(item.id)}
                onDelete={async () => {
                  setInline(null);
                  await props.onDelete(item.id);
                }}
              />
            )}
          </div>
        ))}
      </section>
    </div>
  );
}

function EmptyState({ filter, onNew }: { filter: Filter; onNew: (usage: BuildUpUsage) => void }) {
  const { t } = useI18n();
  const first = useRef<HTMLButtonElement>(null);
  useEffect(() => first.current?.focus(), []);
  if (filter !== "all") {
    return (
      <p className="empty-note">
        {t(filter === "wall" ? "buildups.emptyWall" : "buildups.emptySlab")}
      </p>
    );
  }
  return (
    <div className="empty-state">
      <p className="empty-note">{t("buildups.empty")}</p>
      <div className="empty-actions">
        <button
          type="button"
          className="button button-primary"
          ref={first}
          onClick={() => onNew("wall")}
        >
          {t("buildups.action.newWall")}
        </button>
        <button type="button" className="button" onClick={() => onNew("slab")}>
          {t("buildups.action.newSlab")}
        </button>
      </div>
    </div>
  );
}

/** The refusal of a name, in words, or null when `error` is about something else. */
function nameMessage(error: unknown): MessageKey | null {
  const code = buildUpRefusal(error)?.code;
  return code === "invalid_name" || code === "duplicate_name" ? failureMessage(error) : null;
}

function useNameForm(initial: string, submit: (name: string) => Promise<void>, onDone: () => void) {
  const [name, setName] = useState(initial);
  const [error, setError] = useState<MessageKey | null>(null);
  const [busy, setBusy] = useState(false);
  const run = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await submit(name.trim());
      onDone();
    } catch (failure) {
      setError(nameMessage(failure) ?? "buildups.error.generic");
    } finally {
      setBusy(false);
    }
  };
  return {
    name,
    setName: (value: string) => {
      setName(value);
      setError(null);
    },
    error,
    run,
  };
}

/** A name field in the row, for renaming or for the copy's name: Enter saves, Esc cancels. */
function NameForm({
  label,
  initial,
  submit,
  onDone,
  confirmLabel,
}: {
  label: string;
  initial: string;
  submit: (name: string) => Promise<void>;
  onDone: () => void;
  /** With a label, the form also has Cancel and confirm buttons (the duplicate form). */
  confirmLabel?: string;
}) {
  const { t } = useI18n();
  const id = useId();
  const form = useNameForm(initial, submit, onDone);
  const field = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const input = field.current?.querySelector("input");
    input?.focus();
    input?.select();
  }, []);
  return (
    <div ref={field} className="inline-field">
      <TextField
        id={id}
        label={label}
        value={form.name}
        onChange={form.setName}
        error={form.error === null ? undefined : t(form.error)}
        inputMode="text"
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            void form.run();
          } else if (event.key === "Escape") {
            event.preventDefault();
            onDone();
          }
        }}
      />
      {confirmLabel !== undefined && (
        <div className="dialog-actions">
          <button type="button" className="button button-small" onClick={onDone}>
            {t("buildups.action.cancel")}
          </button>
          <button
            type="button"
            className="button button-small button-primary"
            onClick={() => void form.run()}
          >
            {confirmLabel}
          </button>
        </div>
      )}
    </div>
  );
}

function DeleteConfirmation({
  item,
  onKeep,
  onDelete,
}: {
  item: BuildUpSummary;
  onKeep: () => void;
  onDelete: () => void;
}) {
  const { t, language } = useI18n();
  const titleId = useId();
  const keep = useRef<HTMLButtonElement>(null);
  useEffect(() => keep.current?.focus(), []);
  return (
    <div
      className="dialog-confirm inline-confirm"
      role="alertdialog"
      aria-labelledby={titleId}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          onKeep();
        }
      }}
    >
      <p id={titleId} className="confirm-title">
        {t("buildups.delete.title", { name: item.name })}
      </p>
      <p>
        {t(pluralKey(language, item.layer_count, "buildups.delete.body"), {
          count: item.layer_count,
        })}
      </p>
      <div className="dialog-actions">
        <button type="button" className="button" ref={keep} onClick={onKeep}>
          {t("buildups.delete.keep")}
        </button>
        <button type="button" className="button button-danger" onClick={onDelete}>
          {t("buildups.delete.confirm")}
        </button>
      </div>
    </div>
  );
}
