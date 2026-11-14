import { formatNumber, LENGTH_DECIMALS } from "../georef/units";
import type { Language, MessageKey, MessageParams } from "../i18n/i18n";
import type { AnnouncementKey, PreviewLabel } from "./types";

type Translate = (key: MessageKey, params?: MessageParams) => string;

/** `metres` with the 3 decimals of the 3D view and the locale's separator (FR: "2,500"). */
export function formatMetres(language: Language, metres: number): string {
  return formatNumber(metres, LENGTH_DECIMALS, language);
}

const number = (value: string | number | undefined) => (typeof value === "number" ? value : 0);

/**
 * The live-region text of a viewport announcement (design/ux-notes.md): keys and raw values come
 * from the viewport, words and number formats from the language.
 */
export function announcementText(
  t: Translate,
  language: Language,
  key: AnnouncementKey,
  values: Readonly<Record<string, string | number>>,
): string {
  const metres = (name: string) => formatMetres(language, number(values[name]));
  const { count: rawCount, name, projection } = values;
  switch (key) {
    case "selection.count": {
      const count = number(rawCount);
      if (count === 0) return t("viewport.announce.selection.cleared");
      return count === 1
        ? t("viewport.announce.selection.one")
        : t("viewport.announce.selection.other", { count });
    }
    case "move.distance":
      return t("viewport.announce.move.distance", { distance: metres("distance") });
    case "place.size":
      return t("viewport.announce.place.size", { width: metres("width"), depth: metres("depth") });
    case "place.height":
      return t("viewport.announce.place.height", { height: metres("height") });
    case "section_box.face_moved":
      return t("viewport.announce.section_box.face_moved", { value: metres("value") });
    case "view.preset":
      return t("viewport.announce.view.preset", {
        name: t(`viewport.preset.${String(name)}` as MessageKey),
      });
    case "view.projection":
      return t("viewport.announce.view.projection", {
        projection: t(`viewport.projection.${String(projection)}` as MessageKey),
      });
    default:
      return t(`viewport.announce.${key}`, values);
  }
}

/** The text of the chip next to the pointer during a move or a placement. */
export function previewText(t: Translate, language: Language, label: PreviewLabel): string {
  const value = (name: string) => formatMetres(language, label.values[name] ?? 0);
  switch (label.key) {
    case "move.distance":
      return t("viewport.chip.distance", { distance: value("distance") });
    case "place.size":
    case "place.depth":
      return t("viewport.chip.size", { width: value("width"), depth: value("depth") });
    case "place.height":
      return t("viewport.chip.height", { height: value("height") });
  }
}
