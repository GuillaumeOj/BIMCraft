import { useCallback, useEffect, useId, useRef, useState } from "react";
import {
  deleteMaterial,
  type EntityId,
  type MaterialCategory,
  materialRefusal,
  useApi,
} from "../api";
import { useI18n } from "../i18n/I18nProvider";
import type { MessageKey } from "../i18n/i18n";
import { useShellState } from "../shell/ShellState";
import { Banner } from "../ui/Banner";
import { Dialog } from "../ui/Dialog";
import { DeleteConfirmation } from "./DeleteConfirmation";
import { LibraryBrowser } from "./LibraryBrowser";
import { MaterialDetail } from "./MaterialDetail";
import { failureMessage } from "./messages";
import { NewMaterialForm } from "./NewMaterialForm";
import { ProjectPane } from "./ProjectPane";
import { useMaterialsData } from "./useMaterialsData";
import "./materials.css";

type View = "detail" | "library" | "new";

/** The Materials dialog (F1): the project's materials, the library and the values of each. */
export function MaterialsDialog({ onClose }: { onClose: () => void }) {
  const { t } = useI18n();
  const api = useApi();
  const { notify } = useShellState();
  const titleId = useId();
  const [search, setSearch] = useState("");
  const [category, setCategory] = useState<MaterialCategory | null>(null);
  const [selected, setSelected] = useState<EntityId | null>(null);
  const [view, setView] = useState<View>("detail");
  const [banner, setBanner] = useState<MessageKey | null>(null);
  const [renaming, setRenaming] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [inUse, setInUse] = useState<number | null>(null);
  const [announcement, setAnnouncement] = useState("");
  const listRef = useRef<HTMLDivElement>(null);
  const libraryButton = useRef<HTMLButtonElement>(null);
  const newButton = useRef<HTMLButtonElement>(null);
  const focusList = useRef(false);
  // A material just added: kept selected until the list that contains it has arrived.
  const pendingSelection = useRef<EntityId | null>(null);
  const opened = useRef(false);

  const fail = useCallback((error: unknown) => setBanner(failureMessage(error)), []);
  const { query, list, detail, reload } = useMaterialsData(search, category, selected, fail);

  const items = list.status === "ready" ? list.value : [];
  const empty =
    list.status === "ready" && items.length === 0 && query.trim() === "" && category === null;
  // An empty project shows the library rather than a blank pane, unless a sub-view is open.
  const shownView: View = empty && view === "detail" ? "library" : view;

  // The first row is selected when the dialog opens, and the next one after a deletion.
  useEffect(() => {
    if (list.status !== "ready") return;
    if (pendingSelection.current !== null) {
      if (!list.value.some((item) => item.id === pendingSelection.current)) return;
      pendingSelection.current = null;
    }
    if (selected === null || !list.value.some((item) => item.id === selected)) {
      setSelected(list.value[0]?.id ?? null);
    }
  }, [list, selected]);

  // Focus: the search field, or the library when the project has no material (UX F1 step 1).
  useEffect(() => {
    if (list.status !== "ready" || opened.current) return;
    opened.current = true;
    if (!empty) document.getElementById(`${titleId}-search`)?.focus();
  }, [list, empty, titleId]);

  // After an add or a deletion the list takes the focus once the detail view is back.
  useEffect(() => {
    if (focusList.current && view === "detail" && listRef.current) {
      focusList.current = false;
      listRef.current?.focus();
    }
  });

  const select = (id: EntityId) => {
    setBanner(null);
    setSelected(id);
    setInUse(null);
    setRenaming(false);
  };

  const created = (id: EntityId, name: string, undoable: boolean) => {
    setBanner(null);
    setSearch("");
    setCategory(null);
    pendingSelection.current = id;
    setSelected(id);
    setView("detail");
    focusList.current = true;
    reload();
    if (undoable) notify(t("materials.toast.added", { name }), { undoable: true });
  };

  const startDelete = () => {
    const current = items.find((item) => item.id === selected);
    // The confirmation names and deletes `detail`: wait until it is the selected material's.
    if (current === undefined || detail?.id !== selected) return;
    if (current.used_by > 0) setInUse(current.used_by);
    else setConfirming(true);
  };

  const confirmDelete = async () => {
    if (detail === null) return;
    setConfirming(false);
    const index = items.findIndex((item) => item.id === detail.id);
    try {
      await deleteMaterial(api, detail.id);
      setBanner(null);
      setSelected((items[index + 1] ?? items[index - 1])?.id ?? null);
      focusList.current = true;
      reload();
      notify(t("materials.toast.deleted", { name: detail.name }), { undoable: true });
    } catch (error) {
      const refusal = materialRefusal(error);
      if (refusal?.reason === "in_use") setInUse(refusal.usedBy ?? 1);
      else fail(error);
    }
  };

  // Going back from a sub-view focuses the control that opened it (UX focus order).
  const leaveSubview = () => {
    (view === "library" ? libraryButton : newButton).current?.focus();
    setView("detail");
  };

  // Esc closes the innermost thing first: the confirmation, the sub-view, then the dialog.
  const cancel = () => {
    if (confirming) setConfirming(false);
    else if (view !== "detail") leaveSubview();
    else onClose();
  };

  const backLabel =
    detail === null
      ? t("materials.library.back")
      : t("materials.library.backTo", { name: detail.name });

  return (
    <Dialog titleId={titleId} onCancel={cancel}>
      <h2 id={titleId} className="dialog-title">
        {t("materials.title")}
      </h2>
      {banner !== null && <Banner>{t(banner)}</Banner>}
      <div className="materials-body" inert={confirming}>
        <ProjectPane
          searchId={`${titleId}-search`}
          search={search}
          setSearch={setSearch}
          query={query}
          category={category}
          setCategory={setCategory}
          list={list}
          empty={empty}
          selected={selected}
          onSelect={select}
          onDelete={startDelete}
          onRename={() => setRenaming(true)}
          onLibrary={() => setView("library")}
          onNew={() => setView("new")}
          listRef={listRef}
          libraryRef={libraryButton}
          newRef={newButton}
        />
        <div className="materials-right">
          {shownView === "library" && (
            <LibraryBrowser
              back={empty ? null : { label: backLabel, onBack: leaveSubview }}
              onAdded={(id, name) => created(id, name, true)}
              onFailure={fail}
            />
          )}
          {shownView === "new" && (
            <NewMaterialForm
              backLabel={backLabel}
              onBack={leaveSubview}
              onCreated={(id, name) => created(id, name, false)}
              onFailure={fail}
            />
          )}
          {shownView === "detail" && detail !== null && (
            <MaterialDetail
              // Another material starts on the Mechanical tab, with its menu closed.
              key={detail.id}
              material={detail}
              renaming={renaming}
              setRenaming={setRenaming}
              inUse={inUse}
              onDelete={startDelete}
              onFailure={fail}
              announce={setAnnouncement}
            />
          )}
        </div>
      </div>
      <div className="visually-hidden" role="status">
        {announcement}
      </div>
      <div className="dialog-footer">
        {confirming && detail !== null ? (
          <DeleteConfirmation
            name={detail.name}
            onKeep={() => {
              setConfirming(false);
              listRef.current?.focus();
            }}
            onDelete={() => void confirmDelete()}
          />
        ) : (
          <div className="dialog-actions">
            <button type="button" className="button button-primary" onClick={onClose}>
              {t("materials.action.done")}
            </button>
          </div>
        )}
      </div>
    </Dialog>
  );
}
