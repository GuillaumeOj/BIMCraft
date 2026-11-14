import { type ReactNode, useState } from "react";
import { useI18n } from "../i18n/I18nProvider";
import type { MessageKey } from "../i18n/i18n";
import { Disclosure } from "../ui/Disclosure";
import { SegmentedToggle } from "../ui/SegmentedToggle";
import { ClipPlanesSection } from "./ClipPlanesSection";
import { SectionBoxSection } from "./SectionBoxSection";
import type { Preset, Projection } from "./types";
import { useViewReport } from "./ViewportBridge";
import { VisibilitySection } from "./VisibilitySection";
import { MAX_CLIP_PLANES } from "./viewState";

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

type SectionName = "camera" | "section" | "clip" | "visibility";

/**
 * The View panel (boards 3.1, 3.2, 3.25b, 3.26): the camera, the section box, the clipping planes
 * and the visibility of the 3D view. It renders only what the view reports (UI-01): every control
 * sends a command and shows the state that comes back.
 */
export function ViewPanel() {
  const { t } = useI18n();
  const report = useViewReport();
  const { view, send } = report;
  const [closed, setClosed] = useState<ReadonlySet<SectionName>>(new Set());

  if (view === null) return <p className="empty">{t("view.unavailable")}</p>;

  const section = (name: SectionName, title: MessageKey, status: string, body: ReactNode) => (
    <Disclosure
      title={t(title)}
      status={status}
      open={!closed.has(name)}
      onToggle={(open) =>
        setClosed((previous) => {
          const next = new Set(previous);
          if (open) next.delete(name);
          else next.add(name);
          return next;
        })
      }
    >
      {body}
    </Disclosure>
  );

  const box = view.sectionBox;
  const tags = [
    view.isolating && t("view.tag.isolated"),
    box?.enabled === true && t("view.tag.sectionBox"),
    view.clipPlanes.length > 0 && t("view.tag.planes"),
  ].filter((tag): tag is string => tag !== false);
  const hiddenAny =
    view.isolating ||
    view.hiddenSelection > 0 ||
    [...view.categories, ...view.disciplines].some((group) => group.hidden);

  return (
    <div className="view-panel">
      <div className="view-title">
        <h3>{t("view.title")}</h3>
        {tags.map((tag) => (
          <span className="vp-tag vp-tag-on" key={tag}>
            {tag}
          </span>
        ))}
      </div>
      {section(
        "camera",
        "view.camera",
        t(`viewport.projection.${view.projection}`),
        <div className="view-section">
          <SegmentedToggle<Projection>
            label={t("viewport.projection")}
            value={view.projection}
            options={[
              { value: "perspective", label: t("viewport.projection.perspective") },
              { value: "parallel", label: t("viewport.projection.parallel") },
            ]}
            onChange={(projection) => send({ kind: "projection", projection })}
          />
          <div className="view-row">
            <label htmlFor="view-look-from">{t("view.look")}</label>
            {/* The view does not report which preset the camera is on (orbiting leaves it), so the list is a chooser. */}
            <select
              id="view-look-from"
              className="select"
              value=""
              onChange={(event) => {
                const preset = PRESETS.find((value) => value === event.target.value);
                if (preset !== undefined) send({ kind: "view_preset", preset });
              }}
            >
              <option value="">{t("view.look.choose")}</option>
              {PRESETS.map((preset) => (
                <option key={preset} value={preset}>
                  {t(`viewport.preset.${preset}`)}
                </option>
              ))}
            </select>
          </div>
        </div>,
      )}
      {section(
        "section",
        "view.section",
        t(box?.enabled === true ? "view.section.on" : "view.section.off"),
        <SectionBoxSection report={report} />,
      )}
      {section(
        "clip",
        "view.clip",
        view.clipPlanes.length === 0
          ? t("view.clip.none")
          : t("view.clip.count", { count: view.clipPlanes.length, max: MAX_CLIP_PLANES }),
        <ClipPlanesSection report={report} />,
      )}
      {section(
        "visibility",
        "view.visibility",
        t(hiddenAny ? "view.visibility.changed" : "view.visibility.all"),
        <VisibilitySection report={report} />,
      )}
    </div>
  );
}
