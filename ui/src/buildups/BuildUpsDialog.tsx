import { useCallback, useEffect, useId, useRef, useState } from "react";
import {
  type BuildUpSummary,
  type BuildUpUsage,
  deleteBuildUp,
  duplicateBuildUp,
  getBuildUp,
  listBuildUps,
  updateBuildUp,
  useApi,
} from "../api";
import { useI18n } from "../i18n/I18nProvider";
import type { MessageKey } from "../i18n/i18n";
import { useShellState } from "../shell/ShellState";
import { Banner } from "../ui/Banner";
import { Dialog } from "../ui/Dialog";
import { BuildUpEditorView } from "./BuildUpEditorView";
import { BuildUpList, type Filter } from "./BuildUpList";
import { type Draft, draftFromDetail, newDraft } from "./draft";
import { failureMessage } from "./messages";
import "./buildups.css";

type Editing = { id: string | null; saved: Draft; title: string };

/** The Build-ups dialog (F2): the list of wall and slab build-ups, and their editor. */
export function BuildUpsDialog({ onClose }: { onClose: () => void }) {
  const { t, language } = useI18n();
  const api = useApi();
  const { notify } = useShellState();
  const titleId = useId();
  const [filter, setFilter] = useState<Filter>("all");
  const [items, setItems] = useState<BuildUpSummary[] | null>(null);
  const [loadFailed, setLoadFailed] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [editing, setEditing] = useState<Editing | null>(null);
  const [banner, setBanner] = useState<MessageKey | null>(null);
  const cancelRef = useRef<(() => void) | null>(null);
  const listRef = useRef<HTMLElement>(null);
  const focusRow = useRef<string | null>(null);
  const latest = useRef(0);

  const reload = useCallback(() => {
    latest.current += 1;
    const mine = latest.current;
    listBuildUps(api, filter === "all" ? undefined : filter).then(
      (found) => {
        if (mine !== latest.current) return;
        setItems(found);
        setLoadFailed(false);
      },
      () => mine === latest.current && setLoadFailed(true),
    );
  }, [api, filter]);

  useEffect(reload, [reload]);

  useEffect(() => {
    const unsubscribe = api.onChange(reload).catch(() => undefined);
    return () => {
      void unsubscribe.then((stop) => stop?.());
    };
  }, [api, reload]);

  // The first row is selected when the dialog opens; a row that has gone is replaced.
  useEffect(() => {
    if (items === null || items.some((item) => item.id === selected)) return;
    setSelected(items[0]?.id ?? null);
  }, [items, selected]);

  // After an Apply or a duplication the selected row takes the focus once it is on screen.
  useEffect(() => {
    if (editing !== null || focusRow.current === null) return;
    const row = listRef.current?.querySelector<HTMLElement>(
      `[id$="-${CSS.escape(focusRow.current)}"]`,
    );
    if (row === null || row === undefined) return;
    focusRow.current = null;
    row.focus();
  });

  const fail = (error: unknown) => setBanner(failureMessage(error));

  const openEditor = async (id: string) => {
    try {
      const detail = await getBuildUp(api, id);
      setBanner(null);
      setEditing({
        id,
        saved: draftFromDetail(detail, language),
        title: t("buildups.editor.titleEdit", { name: detail.name }),
      });
    } catch (error) {
      fail(error);
    }
  };

  const openNew = (usage: BuildUpUsage) => {
    setBanner(null);
    setEditing({
      id: null,
      saved: newDraft(usage),
      title: t(usage === "wall" ? "buildups.editor.titleNewWall" : "buildups.editor.titleNewSlab"),
    });
  };

  const closeEditor = (applied: string | null) => {
    setEditing(null);
    if (applied === null) return;
    setSelected(applied);
    focusRow.current = applied;
    reload();
    notify(t("buildups.toast.applied"), { undoable: true });
  };

  const removeBuildUp = async (id: string) => {
    const index = items?.findIndex((item) => item.id === id) ?? -1;
    const next = items?.[index + 1] ?? items?.[index - 1];
    const name = items?.[index]?.name ?? "";
    await deleteBuildUp(api, id);
    setSelected(next?.id ?? null);
    reload();
    notify(t("buildups.toast.deleted", { name }), { undoable: true });
  };

  const copy = async (id: string, name: string) => {
    const created = await duplicateBuildUp(api, id, name);
    setSelected(created);
    focusRow.current = created;
    reload();
  };

  const cancel = () => {
    if (editing !== null) cancelRef.current?.();
    else onClose();
  };

  return (
    <Dialog titleId={titleId} onCancel={cancel}>
      {editing === null ? (
        <>
          <h2 id={titleId} className="dialog-title">
            {t("buildups.title")}
          </h2>
          {banner !== null && <Banner>{t(banner)}</Banner>}
          <div className="dialog-body">
            <BuildUpList
              items={items ?? []}
              loading={items === null && !loadFailed}
              loadFailed={loadFailed}
              onRetry={reload}
              filter={filter}
              onFilter={setFilter}
              selected={selected}
              onSelect={setSelected}
              listRef={listRef}
              onEdit={(id) => void openEditor(id)}
              onNew={openNew}
              onDuplicate={copy}
              onRename={(id, name) => updateBuildUp(api, id, { name })}
              onDelete={removeBuildUp}
            />
          </div>
          <div className="dialog-footer">
            <div className="dialog-actions">
              <button type="button" className="button button-primary" onClick={onClose}>
                {t("buildups.action.close")}
              </button>
            </div>
          </div>
        </>
      ) : (
        <BuildUpEditorView
          titleId={titleId}
          id={editing.id}
          saved={editing.saved}
          title={editing.title}
          onClose={closeEditor}
          cancelRef={cancelRef}
        />
      )}
    </Dialog>
  );
}
