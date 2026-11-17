import type { KeyboardEvent } from "react";
import { formatNumber, LENGTH_DECIMALS } from "../georef/units";
import { useI18n } from "../i18n/I18nProvider";
import { useShellState } from "../shell/ShellState";
import { ClipPlaneIcon, FlipIcon, RemoveIcon } from "./icons";
import type { ViewReport } from "./ViewportBridge";
import { AXIS_NAMES, axisNormal, type ClipPlane, MAX_CLIP_PLANES } from "./viewState";

/**
 * The "Clipping planes" section of the View panel (board 3.25b): one row per plane with Flip and
 * Remove, a button that arms picking a face, and Clear all. Planes are view state, not model
 * history: clearing them offers no Undo.
 */
export function ClipPlanesSection({ report }: { report: ViewReport }) {
  const { t, language } = useI18n();
  const { notify } = useShellState();
  const { view, send, step } = report;
  const planes = view?.clipPlanes ?? [];
  const picking = step === "pick_face";
  const full = planes.length >= MAX_CLIP_PLANES;

  const add = () => {
    if (picking) send({ kind: "cancel" });
    else if (full) notify(t("view.clip.max", { max: MAX_CLIP_PLANES }));
    else send({ kind: "clip_plane_pick" });
  };
  const clear = () => {
    send({ kind: "clip_planes_clear" });
    notify(t("view.clip.cleared"));
  };

  const planeSummary = (plane: ClipPlane) => {
    const along = axisNormal(plane.normal);
    if (along === null) return t("view.clip.oblique");
    const name = AXIS_NAMES[along.axis];
    const offset = formatNumber(plane.point[along.axis], LENGTH_DECIMALS, language);
    return `${t("view.clip.normal", { axis: `${along.sign}${name}` })} · ${t("view.clip.offset", { axis: name.toLowerCase(), value: offset })}`;
  };

  // Delete removes the plane of the row holding the focus (its Flip or Remove button).
  const onKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    if (event.key !== "Delete" || !(event.target instanceof Element)) return;
    const index = Number(event.target.closest("[data-plane]")?.getAttribute("data-plane"));
    if (!Number.isInteger(index)) return;
    event.preventDefault();
    send({ kind: "clip_plane_remove", index });
  };

  return (
    <div className="view-section">
      {planes.length > 0 && (
        <ul className="plane-rows" aria-label={t("view.clip")} onKeyDown={onKeyDown}>
          {planes.map((plane, index) => {
            const name = t("view.clip.plane", { n: index + 1 });
            return (
              <li
                // Planes have no identity: a row is its position, as for the commands.
                // biome-ignore lint/suspicious/noArrayIndexKey: see above.
                key={index}
                className="plane-row"
                data-plane={index}
              >
                <ClipPlaneIcon />
                <span className="plane-name">{name}</span>
                <span className="plane-detail">{planeSummary(plane)}</span>
                <button
                  type="button"
                  className="vp-icon-button"
                  aria-label={t("view.clip.flip", { n: index + 1 })}
                  title={t("view.clip.flip", { n: index + 1 })}
                  onClick={() => send({ kind: "clip_plane_flip", index })}
                >
                  <FlipIcon />
                </button>
                <button
                  type="button"
                  className="vp-icon-button"
                  aria-label={t("view.clip.remove", { n: index + 1 })}
                  title={t("view.clip.remove", { n: index + 1 })}
                  onClick={() => send({ kind: "clip_plane_remove", index })}
                >
                  <RemoveIcon />
                </button>
              </li>
            );
          })}
        </ul>
      )}
      <div className="view-buttons">
        <button
          type="button"
          className="button button-small"
          aria-pressed={picking}
          aria-disabled={view === null || (full && !picking)}
          onClick={add}
        >
          {t("view.clip.add")}
        </button>
        {planes.length > 0 && (
          <button type="button" className="ghost-button" onClick={clear}>
            {t("view.clip.clear")}
          </button>
        )}
      </div>
    </div>
  );
}
