import { type MutableRefObject, useEffect, useMemo, useRef, useState } from "react";
import {
  type ApiClient,
  type BuildUpLayerInput,
  buildUpRefusal,
  createBuildUp,
  type EntityId,
  type MaterialSummary,
  previewBuildUp,
  updateBuildUp,
  useApi,
} from "../api";
import { useI18n } from "../i18n/I18nProvider";
import { MaterialsDialog } from "../materials/MaterialsDialog";
import { Banner } from "../ui/Banner";
import { ErrorIcon } from "../ui/ErrorIcon";
import { BuildUpEditor } from "./BuildUpEditor";
import { type Draft, isDirty, layerInputs } from "./draft";
import { buildErrors, type CoreError, noErrors } from "./errors";
import { pluralKey } from "./messages";
import { useBuildUpPreview } from "./useBuildUpPreview";
import { useProjectMaterials } from "./useProjectMaterials";

type Props = {
  titleId: string;
  /** The saved build-up being edited, or null for a new one. */
  id: EntityId | null;
  saved: Draft;
  title: string;
  /** Called when the editor closes: with the id after Apply, or null when cancelled. */
  onClose: (applied: EntityId | null) => void;
  /** The dialog's Esc calls this: it asks to discard when there are changes. */
  cancelRef: MutableRefObject<(() => void) | null>;
};

/** The build-up editor inside the dialog: draft, live preview, Apply, discard and materials. */
export function BuildUpEditorView({ titleId, id, saved, title, onClose, cancelRef }: Props) {
  const { t, language } = useI18n();
  const api = useApi();
  const [draft, setDraft] = useState(saved);
  const [submitted, setSubmitted] = useState(false);
  const [refusals, setRefusals] = useState<CoreError[]>([]);
  const [confirming, setConfirming] = useState(false);
  const [managing, setManaging] = useState<string | null>(null);
  const [live, setLive] = useState({ text: "", n: 0 });
  const [busy, setBusy] = useState(false);
  const applying = useRef(false);
  const { materials, revision, reload } = useProjectMaterials();
  const before = useRef<ReadonlySet<string>>(new Set());
  const body = useRef<HTMLFormElement>(null);

  const preview = useBuildUpPreview(draft, revision);
  const dirty = useMemo(() => isDirty(draft, saved), [draft, saved]);
  const errors = submitted
    ? buildErrors(draft, [...(preview?.errors ?? []), ...refusals])
    : noErrors(draft);

  const announce = (text: string) => setLive((current) => ({ text, n: current.n + 1 }));

  // A new build-up starts in the Name field, an existing one on its first layer (UX focus order).
  useEffect(() => {
    const first = id === null ? "input" : "[data-control=function]";
    body.current?.querySelector<HTMLElement>(first)?.focus();
  }, [id]);

  // The confirmation is gone: focus goes back to the form so that Esc still works.
  const keepEditing = () => {
    setConfirming(false);
    body.current?.querySelector<HTMLElement>("input")?.focus();
  };

  const requestCancel = () => {
    if (confirming) keepEditing();
    else if (dirty) setConfirming(true);
    else onClose(null);
  };
  cancelRef.current = requestCancel;

  const edit = (next: Draft) => {
    setDraft(next);
    setRefusals([]);
  };

  const focusFirstProblem = () =>
    body.current?.querySelector<HTMLElement>("[aria-invalid=true]")?.focus();

  /** The messages that stop Apply; the core is asked about this very draft, not the last one. */
  const problems = async (layers: BuildUpLayerInput[] | null) => {
    const answer =
      layers === null ? null : await previewBuildUp(api, draft.usage, layers).catch(() => null);
    return { answer, found: buildErrors(draft, answer?.errors ?? []) };
  };

  const apply = async () => {
    if (applying.current) return;
    applying.current = true;
    setBusy(true);
    try {
      await check();
    } finally {
      applying.current = false;
      setBusy(false);
    }
  };

  const check = async () => {
    setSubmitted(true);
    const layers = layerInputs(draft);
    const { answer, found } = await problems(layers);
    if (found.count > 0 || layers === null) {
      // Show what the core just said, whether or not the live preview has caught up.
      setRefusals(answer?.errors ?? []);
      announce(
        t(pluralKey(language, found.count, "buildups.error.summary"), { count: found.count }),
      );
      // The messages render first; focus follows them.
      setTimeout(focusFirstProblem);
    } else if (!dirty) {
      onClose(null);
    } else {
      await commit(layers);
    }
  };

  const commit = async (layers: BuildUpLayerInput[]) => {
    try {
      onClose(await save(api, id, draft, layers));
    } catch (error) {
      const refusal = buildUpRefusal(error);
      setRefusals([{ code: refusal?.code ?? "", index: refusal?.index ?? undefined }]);
      setTimeout(focusFirstProblem);
    }
  };

  const manage = (key: string) => {
    before.current = new Set(materials.map((material) => material.id));
    setManaging(key);
  };

  // Closing the Materials dialog selects the material it added, if any, for the layer.
  const doneManaging = async () => {
    const key = managing;
    setManaging(null);
    const fresh: readonly MaterialSummary[] | null = await reload();
    const added = fresh?.find((material) => !before.current.has(material.id));
    if (added === undefined || key === null) return;
    setDraft((current) => ({
      ...current,
      layers: current.layers.map((layer) =>
        layer.key === key ? { ...layer, material: added.id } : layer,
      ),
    }));
  };

  const subtitle = t(
    draft.usage === "wall" ? "buildups.editor.subtitleWall" : "buildups.editor.subtitleSlab",
  );

  return (
    <>
      <div className="editor-header">
        <h2 id={titleId} className="dialog-title">
          {title}
        </h2>
        <p className="editor-subtitle">{subtitle}</p>
      </div>
      {errors.banner !== null && <Banner>{t(errors.banner)}</Banner>}
      <form
        className="editor-body"
        ref={body}
        inert={confirming}
        onSubmit={(event) => {
          event.preventDefault();
          void apply();
        }}
      >
        <BuildUpEditor
          draft={draft}
          onChange={edit}
          materials={materials}
          preview={preview}
          errors={errors}
          onManageMaterials={manage}
          announce={announce}
        />
        <button type="submit" hidden />
      </form>
      <div key={live.n} className="visually-hidden" role="status" aria-live="polite">
        {live.text}
      </div>
      <div className="dialog-footer">
        {confirming ? (
          <DiscardConfirmation onKeep={keepEditing} onDiscard={() => onClose(null)} />
        ) : (
          <>
            <p className="footer-note" data-error={submitted && errors.count > 0}>
              {submitted && errors.count > 0 ? (
                <>
                  <ErrorIcon />
                  {t(pluralKey(language, errors.count, "buildups.error.summary"), {
                    count: errors.count,
                  })}
                </>
              ) : (
                t("buildups.footer.note")
              )}
            </p>
            <div className="dialog-actions">
              <button type="button" className="button" onClick={requestCancel}>
                {t("buildups.action.cancel")}
              </button>
              <button
                type="button"
                className="button button-primary"
                disabled={busy}
                onClick={() => void apply()}
              >
                {t("buildups.action.apply")}
              </button>
            </div>
          </>
        )}
      </div>
      {managing !== null && <MaterialsDialog onClose={() => void doneManaging()} />}
    </>
  );
}

function DiscardConfirmation({ onKeep, onDiscard }: { onKeep: () => void; onDiscard: () => void }) {
  const { t } = useI18n();
  const keep = useRef<HTMLButtonElement>(null);
  useEffect(() => keep.current?.focus(), []);
  return (
    <div className="dialog-confirm" role="alertdialog" aria-label={t("buildups.discard.title")}>
      <p className="confirm-title">{t("buildups.discard.title")}</p>
      <div className="dialog-actions">
        <button type="button" className="button" ref={keep} onClick={onKeep}>
          {t("buildups.discard.keep")}
        </button>
        <button type="button" className="button button-danger" onClick={onDiscard}>
          {t("buildups.discard.confirm")}
        </button>
      </div>
    </div>
  );
}

/** Creates or updates the build-up in one undoable step; resolves to its id. */
async function save(
  api: ApiClient,
  id: EntityId | null,
  draft: Draft,
  layers: BuildUpLayerInput[],
): Promise<EntityId> {
  const name = draft.name.trim();
  if (id === null) {
    return createBuildUp(api, { name, usage: draft.usage, description: draft.description, layers });
  }
  await updateBuildUp(api, id, { name, description: draft.description || null, layers });
  return id;
}
