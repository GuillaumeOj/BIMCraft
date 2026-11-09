import { useCallback, useEffect, useId, useRef, useState } from "react";
import { apiReason, useApi } from "../api";
import { useI18n } from "../i18n/I18nProvider";
import type { MessageKey } from "../i18n/i18n";
import { useShellState } from "../shell/ShellState";
import { Banner } from "../ui/Banner";
import { CoordinateBlock } from "./CoordinateBlock";
import { loadErrorMessage } from "./formModel";
import { type GeorefInfo, useGeoref } from "./GeorefContext";
import {
  BLOCK_TITLE,
  copyText,
  formatPoint,
  type GeographicFormat,
  type Point,
  parsePoint,
  SYSTEMS,
  type System,
  type Texts,
} from "./pointFormat";

/** A point from outside the panel, such as one picked in a view: it is entered as if typed. */
export type PickedPoint = { system: System; point: Point };

const DEBOUNCE_MS = 200;
const ORIGIN: Point = [0, 0, 0];
const NO_ERRORS: [boolean, boolean, boolean] = [false, false, false];

type Blocks = Record<System, Texts>;
const BLANK: Texts = ["", "", ""];

/** The panel body for a georeferenced project: one point, shown in the three systems. */
function Converter({ info, pickedPoint }: { info: GeorefInfo; pickedPoint: PickedPoint | null }) {
  const api = useApi();
  const { t, language } = useI18n();
  const { notify } = useShellState();
  const tooltipId = useId();
  const [format, setFormat] = useState<GeographicFormat>("decimal");
  const [texts, setTexts] = useState<Blocks>({
    local: formatPoint("local", ORIGIN, "decimal", language),
    projected: BLANK,
    geographic: BLANK,
  });
  const [invalid, setInvalid] = useState<Record<System, [boolean, boolean, boolean]>>({
    local: NO_ERRORS,
    projected: NO_ERRORS,
    geographic: NO_ERRORS,
  });
  const [source, setSource] = useState<System | null>(null);
  const [error, setError] = useState<MessageKey | null>(null);
  // The last converted point in each system, for Copy and for changing the format.
  const points = useRef<Partial<Record<System, Point>>>({ local: ORIGIN });
  const latest = useRef(0);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const formatRef = useRef(format);
  formatRef.current = format;

  const convert = useCallback(
    async (from: System, point: Point, typed: System | null) => {
      latest.current += 1;
      const mine = latest.current;
      try {
        const { points: result } = await api.query("georef.convert", { from, points: [point] });
        const converted = result[0];
        if (mine !== latest.current || converted === undefined) return;
        points.current = {
          local: converted.local,
          projected: converted.projected,
          geographic: converted.geographic,
        };
        setError(null);
        // A block that was flagged invalid now shows computed values again.
        setInvalid((current) => ({
          local: typed === "local" ? current.local : NO_ERRORS,
          projected: typed === "projected" ? current.projected : NO_ERRORS,
          geographic: typed === "geographic" ? current.geographic : NO_ERRORS,
        }));
        setTexts((current) => {
          const next = { ...current };
          for (const system of SYSTEMS) {
            // The block being typed in keeps its text, as typed.
            if (system !== typed) {
              next[system] = formatPoint(system, converted[system], formatRef.current, language);
            }
          }
          return next;
        });
      } catch (failure) {
        if (mine !== latest.current) return;
        points.current = {};
        const outside = apiReason(failure) === "outside_domain";
        setError(outside ? "georef.convert.outside" : "georef.convert.failed");
        setTexts((current) => {
          const next = { ...current };
          for (const system of SYSTEMS) if (system !== typed) next[system] = BLANK;
          return next;
        });
      }
    },
    [api, language],
  );

  // After the georeference changes (apply, remove, undo), the current local point is converted
  // again; the point itself stays.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `info` is the trigger
  useEffect(() => {
    setSource(null);
    void convert("local", points.current.local ?? ORIGIN, null);
  }, [info, convert]);

  useEffect(() => {
    if (pickedPoint === null) return;
    setSource(pickedPoint.system);
    setTexts((current) => ({
      ...current,
      [pickedPoint.system]: formatPoint(
        pickedPoint.system,
        pickedPoint.point,
        formatRef.current,
        language,
      ),
    }));
    setInvalid((current) => ({ ...current, [pickedPoint.system]: NO_ERRORS }));
    void convert(pickedPoint.system, pickedPoint.point, pickedPoint.system);
  }, [pickedPoint, convert, language]);

  useEffect(() => () => clearTimeout(timer.current), []);

  const flush = (system: System, blockTexts: Texts) => {
    clearTimeout(timer.current);
    const parsed = parsePoint(system, blockTexts);
    setInvalid((current) => ({ ...current, [system]: parsed.invalid }));
    if (parsed.point !== null) void convert(system, parsed.point, system);
  };

  const edit = (system: System, index: number, text: string) => {
    const block = [...texts[system]] as Texts;
    block[index] = text;
    setTexts((current) => ({ ...current, [system]: block }));
    setSource(system);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => flush(system, block), DEBOUNCE_MS);
  };

  const changeFormat = (next: GeographicFormat) => {
    setFormat(next);
    formatRef.current = next;
    const geographic = points.current.geographic;
    if (geographic !== undefined) {
      setTexts((current) => ({
        ...current,
        geographic: formatPoint("geographic", geographic, next, language),
      }));
    }
  };

  const copy = async (system: System) => {
    const point = points.current[system];
    if (point === undefined) return;
    try {
      await navigator.clipboard.writeText(copyText(system, point, format, language));
      notify(t("georef.toast.copied"));
    } catch {
      // Clipboard access can be refused; nothing was copied, so nothing is announced.
    }
  };

  const reset = () => {
    clearTimeout(timer.current);
    points.current = { local: ORIGIN };
    setSource(null);
    setInvalid({ local: NO_ERRORS, projected: NO_ERRORS, geographic: NO_ERRORS });
    setTexts((current) => ({
      ...current,
      local: formatPoint("local", ORIGIN, "decimal", language),
    }));
    void convert("local", ORIGIN, null);
  };

  const captions: Record<System, string> = {
    local: t("georef.block.localCaption"),
    projected: info.crs.name,
    geographic: info.crs.geographic_name,
  };

  return (
    <>
      <div className="panel-crs">
        <p>{`${info.crs.name} (EPSG ${info.crs.epsg})`}</p>
        <div className="panel-actions">
          <span className="tooltip-anchor">
            <button
              type="button"
              className="button button-small"
              aria-disabled="true"
              aria-describedby={tooltipId}
              onClick={(event) => event.preventDefault()}
            >
              {t("georef.panel.pick")}
            </button>
            <span id={tooltipId} role="tooltip" className="tooltip">
              {t("georef.panel.pickDisabled")}
            </span>
          </span>
          <OpenLocation />
          <button type="button" className="button button-small" onClick={reset}>
            {t("georef.panel.reset")}
          </button>
        </div>
      </div>
      {SYSTEMS.map((system) => (
        <CoordinateBlock
          key={system}
          system={system}
          caption={captions[system]}
          texts={texts[system]}
          invalid={invalid[system]}
          isSource={source === system}
          computedFrom={source !== null && source !== system ? t(BLOCK_TITLE[source]) : null}
          error={
            error !== null && (system !== source || error === "georef.convert.failed")
              ? t(error)
              : null
          }
          format={format}
          onFormat={changeFormat}
          canCopy={points.current[system] !== undefined}
          onEdit={(index, text) => edit(system, index, text)}
          onEnter={() => flush(system, texts[system])}
          onCopy={() => void copy(system)}
        />
      ))}
      <div className="visually-hidden" role="status">
        {error === null ? "" : t(error)}
      </div>
    </>
  );
}

function OpenLocation({ label = "georef.panel.change" }: { label?: MessageKey }) {
  const { t } = useI18n();
  const { openDialog } = useGeoref();
  return (
    <button type="button" className="button button-small" onClick={openDialog}>
      {t(label)}
    </button>
  );
}

/**
 * The Coordinates panel (F2): a point in model, map and latitude/longitude coordinates at once.
 * Typing in any block converts to the other two. `pickedPoint` lets a future viewport push a
 * point into it.
 */
export function CoordinatesPanel({ pickedPoint = null }: { pickedPoint?: PickedPoint | null }) {
  const { t } = useI18n();
  const { state } = useGeoref();

  if (state.status === "loading") return <p className="empty">{t("georef.loading")}</p>;
  if (state.status === "error") {
    return (
      <div className="panel-note">
        <Banner>{t(loadErrorMessage(state.reason))}</Banner>
      </div>
    );
  }
  if (state.info === null) {
    return (
      <div className="panel-note">
        <p>{t("georef.panel.empty")}</p>
        <OpenLocation label="georef.panel.setLocation" />
      </div>
    );
  }
  return (
    <>
      {state.reloadError !== null && (
        <div className="panel-note">
          <Banner>{t(loadErrorMessage(state.reloadError.reason))}</Banner>
        </div>
      )}
      <Converter info={state.info} pickedPoint={pickedPoint} />
    </>
  );
}
