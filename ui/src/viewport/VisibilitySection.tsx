import { useI18n } from "../i18n/I18nProvider";
import type { ViewReport } from "./ViewportBridge";
import { hiddenNames, isolatedNames } from "./viewState";

/**
 * The "Visibility" section of the View panel (board 3.2): what is hidden and what is isolated, as
 * the viewport reports it, with End isolate and Reset visibility.
 */
export function VisibilitySection({ report }: { report: ViewReport }) {
  const { t } = useI18n();
  const { view, send } = report;
  if (view === null) return null;
  const hiddenCategories = hiddenNames(view.categories);
  const hiddenDisciplines = hiddenNames(view.disciplines);
  const isolated = [...isolatedNames(view.categories), ...isolatedNames(view.disciplines)];
  const rows: [string, string][] = [];
  if (hiddenCategories.length > 0) {
    rows.push([t("view.visibility.hiddenCategories"), hiddenCategories.join(", ")]);
  }
  if (hiddenDisciplines.length > 0) {
    rows.push([t("view.visibility.hiddenDisciplines"), hiddenDisciplines.join(", ")]);
  }
  if (view.hiddenSelection > 0) {
    rows.push([t("view.visibility.hiddenSelection"), String(view.hiddenSelection)]);
  }
  if (view.isolating) {
    rows.push([
      t("view.visibility.isolated"),
      isolated.length > 0 ? isolated.join(", ") : t("view.visibility.theSelection"),
    ]);
  }

  return (
    <div className="view-section">
      {rows.length === 0 ? (
        <p className="empty view-empty">{t("view.visibility.all")}</p>
      ) : (
        <dl className="view-facts">
          {rows.map(([name, value]) => (
            <div className="view-fact" key={name}>
              <dt>{name}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
      )}
      <div className="view-buttons">
        {view.isolating && (
          <button
            type="button"
            className="button button-small"
            onClick={() => send({ kind: "end_isolate" })}
          >
            {t("view.visibility.end")}
          </button>
        )}
        <button
          type="button"
          className="ghost-button"
          aria-keyshortcuts="Alt+H"
          title={t("ui.withShortcut", { label: t("view.visibility.reset"), shortcut: "Alt+H" })}
          onClick={() => send({ kind: "reset_visibility" })}
        >
          {t("view.visibility.reset")}
        </button>
      </div>
    </div>
  );
}
