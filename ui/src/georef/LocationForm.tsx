import { type FormEvent, type KeyboardEvent, useEffect, useId, useRef, useState } from "react";
import { apiReason, setGeoreference, useApi } from "../api";
import { useI18n } from "../i18n/I18nProvider";
import type { MessageKey } from "../i18n/i18n";
import { Banner } from "../ui/Banner";
import { Disclosure } from "../ui/Disclosure";
import { ErrorIcon } from "../ui/ErrorIcon";
import { TextField } from "../ui/TextField";
import { CrsCombobox, type CrsList } from "./CrsCombobox";
import { FormatToggle } from "./FormatToggle";
import {
  type FieldErrors,
  type FieldKey,
  type FormValues,
  firstInvalid,
  formatLength,
  formatRotation,
  initialValues,
  reasonMessage,
  type SURVEY_FIELDS,
  toParams,
} from "./formModel";
import { type GeorefInfo, useGeoref } from "./GeorefContext";
import { NorthArrow } from "./NorthArrow";
import type { GeographicFormat } from "./pointFormat";
import {
  ANGLE_DECIMALS,
  DEGREE,
  degToRad,
  formatDms,
  formatNumber,
  formatSigned,
  GEOGRAPHIC_DECIMALS,
  LATITUDE,
  LONGITUDE,
  METRE,
  parseAngle,
  radToDeg,
} from "./units";

const SCALE_DECIMALS = 6;

function SectionHeader({ number, id, title }: { number: number; id: string; title: string }) {
  return (
    <h3 className="section-header" id={id}>
      <span className="section-number" aria-hidden="true">
        {number}
      </span>
      {title}
    </h3>
  );
}

function DerivedInfo({ info, dms }: { info: GeorefInfo | null; dms: boolean }) {
  const { t, language } = useI18n();
  const derived = info?.derived;
  const geographic = (radians: number, hemispheres: typeof LATITUDE) =>
    dms
      ? formatDms(radToDeg(radians), hemispheres, language)
      : `${formatNumber(radToDeg(radians), GEOGRAPHIC_DECIMALS, language)}${DEGREE}`;
  const datum = info?.crs.geographic_name ?? "";
  const [latitude, longitude] = derived?.base_point_geographic ?? [];
  const dash = "–";
  const rows: [string, string][] = [
    [
      t("georef.derived.latitude", { datum }),
      latitude === undefined ? dash : geographic(latitude, LATITUDE),
    ],
    [
      t("georef.derived.longitude", { datum }),
      longitude === undefined ? dash : geographic(longitude, LONGITUDE),
    ],
    [
      t("georef.derived.convergence"),
      derived
        ? `${formatSigned(radToDeg(derived.convergence), ANGLE_DECIMALS, language)}${DEGREE}`
        : dash,
    ],
    [
      t("georef.derived.scaleFactor"),
      derived ? formatNumber(derived.point_scale_factor, SCALE_DECIMALS, language) : dash,
    ],
  ];
  return (
    <dl>
      {rows.map(([label, value]) => (
        <div key={label}>
          <dt>{label}</dt>
          <dd>{value}</dd>
        </div>
      ))}
    </dl>
  );
}

/** The form of the Project location dialog, for the project's current georeference `info`. */
export function LocationForm({ info }: { info: GeorefInfo | null }) {
  const api = useApi();
  const { t, language } = useI18n();
  const { closeDialog, notify } = useGeoref();
  const prefix = useId();
  const fieldId = (key: FieldKey) => `${prefix}-${key}`;
  const baseErrorId = `${prefix}-base-error`;

  const [angleFormat, setAngleFormat] = useState<GeographicFormat>("decimal");
  const [initial] = useState(() => initialValues(info, "decimal", language));
  const [values, setValues] = useState<FormValues>(initial);
  const [errors, setErrors] = useState<FieldErrors>({});
  const [baseError, setBaseError] = useState<MessageKey | null>(null);
  const [banner, setBanner] = useState<MessageKey | null>(null);
  const [summary, setSummary] = useState("");
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [surveyOpen, setSurveyOpen] = useState(info?.value.survey_point != null);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [busy, setBusy] = useState(false);
  const [crsList, setCrsList] = useState<CrsList>({ status: "loading" });
  const [focusRequest, setFocusRequest] = useState<{ key: FieldKey; count: number }>({
    key: info === null ? "crs" : "easting",
    count: 0,
  });
  const keepButton = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    let current = true;
    api.query("georef.list_crs", {}).then(
      ({ crs }) => current && setCrsList({ status: "ready", crs }),
      () => current && setCrsList({ status: "error" }),
    );
    return () => {
      current = false;
    };
  }, [api]);

  // Focus moves after the render that opened the section holding the field.
  useEffect(() => {
    document.getElementById(`${prefix}-${focusRequest.key}`)?.focus();
  }, [focusRequest, prefix]);

  useEffect(() => {
    if (confirmRemove) keepButton.current?.focus();
  }, [confirmRemove]);

  const focusField = (key: FieldKey) => setFocusRequest((last) => ({ key, count: last.count + 1 }));

  const set = (key: keyof FormValues, value: string) => {
    setValues((current) => ({ ...current, [key]: value }));
    setErrors((current) => ({ ...current, [key]: undefined }));
    if (key === "easting" || key === "northing" || key === "height") setBaseError(null);
  };

  const showErrors = (found: FieldErrors) => {
    setErrors(found);
    if (found.scale) setAdvancedOpen(true);
    if (found.surveyEasting || found.surveyNorthing || found.surveyHeight) setSurveyOpen(true);
    setSummary(t("georef.error.summary", { count: Object.keys(found).length }));
    const first = firstInvalid(found);
    if (first !== undefined) focusField(first);
  };

  const refused = (error: unknown) => {
    const reason = apiReason(error);
    if (reason === "unknown_crs" || reason === "not_projected") {
      showErrors({ crs: reasonMessage(reason) });
    } else if (reason === "invalid_scale") {
      showErrors({ scale: "georef.error.invalid_scale" });
    } else if (reason === "outside_domain") {
      setBaseError("georef.error.outside_domain");
      setSummary(t("georef.error.outside_domain"));
      focusField("easting");
    } else {
      setBanner(reasonMessage(reason));
    }
  };

  const apply = async (event: FormEvent) => {
    event.preventDefault();
    if (busy) return;
    setBanner(null);
    setSummary("");
    const result = toParams(
      values,
      info === null ? undefined : { values: initial, value: info.value },
    );
    if ("errors" in result) return showErrors(result.errors);
    if (JSON.stringify(values) === JSON.stringify(initial)) return closeDialog();
    setBusy(true);
    try {
      await setGeoreference(api, result.params);
      closeDialog();
      notify(t("georef.toast.applied"), true);
    } catch (error) {
      refused(error);
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    setBusy(true);
    try {
      await api.command("georef.clear", {});
      closeDialog();
      notify(t("georef.toast.removed"), true);
    } catch (error) {
      setConfirmRemove(false);
      setBanner(reasonMessage(apiReason(error)));
    } finally {
      setBusy(false);
    }
  };

  const changeFormat = (next: GeographicFormat) => {
    const degrees = parseAngle(values.rotation);
    if (degrees !== null) set("rotation", formatRotation(degToRad(degrees), next, language));
    setAngleFormat(next);
  };

  // In the removal confirmation Esc keeps the georeference; the dialog stays open.
  const onKeyDown = (event: KeyboardEvent<HTMLFormElement>) => {
    if (event.key === "Escape" && confirmRemove) {
      event.preventDefault();
      setConfirmRemove(false);
    }
  };

  const empty = values.crs === null;
  const error = (key: FieldKey) => {
    const message = errors[key];
    return message === undefined ? undefined : t(message);
  };
  const surveyField = (key: (typeof SURVEY_FIELDS)[number], label: MessageKey) => (
    <TextField
      id={fieldId(key)}
      label={t(label)}
      value={values[key]}
      unit={METRE}
      disabled={empty}
      error={error(key)}
      onChange={(value) => set(key, value)}
    />
  );
  const baseField = (key: "easting" | "northing" | "height", label: MessageKey) => (
    <TextField
      id={fieldId(key)}
      label={t(label)}
      value={values[key]}
      unit={METRE}
      disabled={empty}
      error={error(key)}
      invalid={baseError !== null && key !== "height"}
      describedBy={`${prefix}-base-help${baseError !== null ? ` ${baseErrorId}` : ""}`}
      onChange={(value) => set(key, value)}
    />
  );

  const derived = info?.derived;
  const dms = angleFormat === "dms";
  const surveyLocal = derived?.survey_point_local;

  return (
    <form className="location-form" noValidate onSubmit={apply} onKeyDown={onKeyDown}>
      <div className="dialog-body" inert={confirmRemove}>
        {info === null && <p className="dialog-intro">{t("georef.location.intro")}</p>}
        {banner !== null && <Banner>{t(banner)}</Banner>}

        <section aria-labelledby={`${prefix}-crs-title`}>
          <SectionHeader number={1} id={`${prefix}-crs-title`} title={t("georef.crs.label")} />
          <p id={`${prefix}-crs-help`} className="section-help">
            {t("georef.crs.help")}
          </p>
          <CrsCombobox
            id={fieldId("crs")}
            labelledBy={`${prefix}-crs-title`}
            list={crsList}
            selected={values.crs}
            onSelect={(crs) => {
              setValues((current) => ({ ...current, crs }));
              setErrors((current) => ({ ...current, crs: undefined }));
              setBaseError(null);
            }}
            describedBy={`${prefix}-crs-help${errors.crs ? ` ${prefix}-crs-error` : ""}`}
            invalid={errors.crs !== undefined}
          />
          {errors.crs && (
            <p id={`${prefix}-crs-error`} className="field-error">
              <ErrorIcon />
              {t(errors.crs)}
            </p>
          )}
          {values.crs && (
            <p className="section-help">{t("georef.crs.area", { area: values.crs.area })}</p>
          )}
        </section>

        <section aria-labelledby={`${prefix}-base-title`}>
          <SectionHeader number={2} id={`${prefix}-base-title`} title={t("georef.base.title")} />
          <p id={`${prefix}-base-help`} className="section-help">
            {t("georef.base.help")}
          </p>
          <div className="field-row">
            {baseField("easting", "georef.base.easting")}
            {baseField("northing", "georef.base.northing")}
            {baseField("height", "georef.base.height")}
          </div>
          {baseError !== null && (
            <p id={baseErrorId} className="field-error">
              <ErrorIcon />
              {t(baseError)}
            </p>
          )}
        </section>

        <section aria-labelledby={`${prefix}-orientation-title`}>
          <SectionHeader
            number={3}
            id={`${prefix}-orientation-title`}
            title={t("georef.orientation.title")}
          />
          <div className="orientation">
            <div>
              <TextField
                id={fieldId("rotation")}
                label={t("georef.rotation.label")}
                help={t("georef.rotation.help")}
                value={values.rotation}
                unit={dms ? undefined : DEGREE}
                disabled={empty}
                error={error("rotation")}
                onChange={(value) => set("rotation", value)}
              />
              <FormatToggle value={angleFormat} onChange={changeFormat} />
            </div>
            <NorthArrow
              gridNorth={info === null ? null : -radToDeg(info.value.grid_rotation)}
              trueNorth={derived === undefined ? null : radToDeg(derived.true_north)}
            />
          </div>
        </section>

        <Disclosure title={t("georef.advanced")} open={advancedOpen} onToggle={setAdvancedOpen}>
          <TextField
            id={fieldId("scale")}
            label={t("georef.scale.label")}
            help={t("georef.scale.help")}
            value={values.scale}
            disabled={empty}
            error={error("scale")}
            onChange={(value) => set("scale", value)}
          />
        </Disclosure>

        <Disclosure title={t("georef.survey.title")} open={surveyOpen} onToggle={setSurveyOpen}>
          <p className="section-help">{t("georef.survey.help")}</p>
          <div className="field-row">
            {surveyField("surveyEasting", "georef.base.easting")}
            {surveyField("surveyNorthing", "georef.base.northing")}
            {surveyField("surveyHeight", "georef.base.height")}
          </div>
          {surveyLocal && (
            <p className="section-help">
              {t("georef.survey.local", {
                x: formatLength(surveyLocal[0], language),
                y: formatLength(surveyLocal[1], language),
                z: formatLength(surveyLocal[2], language),
              })}
            </p>
          )}
        </Disclosure>

        <section className="derived" aria-labelledby={`${prefix}-derived-title`}>
          <h3 id={`${prefix}-derived-title`}>{t("georef.derived.title")}</h3>
          <DerivedInfo info={info} dms={dms} />
        </section>
      </div>

      {confirmRemove ? (
        <section className="dialog-footer dialog-confirm" aria-labelledby={`${prefix}-remove`}>
          <div>
            <p id={`${prefix}-remove`} className="confirm-title">
              {t("georef.remove.title")}
            </p>
            <p>{t("georef.remove.body")}</p>
          </div>
          <div className="dialog-actions">
            <button
              type="button"
              className="button"
              ref={keepButton}
              onClick={() => setConfirmRemove(false)}
            >
              {t("georef.remove.keep")}
            </button>
            <button type="button" className="button button-danger" disabled={busy} onClick={remove}>
              {t("georef.remove.confirm")}
            </button>
          </div>
        </section>
      ) : (
        <div className="dialog-footer">
          {info !== null && (
            <button
              type="button"
              className="button button-danger-quiet"
              onClick={() => setConfirmRemove(true)}
            >
              {t("georef.action.remove")}
            </button>
          )}
          <div className="dialog-actions">
            <button type="button" className="button" onClick={closeDialog}>
              {t("georef.action.cancel")}
            </button>
            <button type="submit" className="button button-primary" disabled={busy}>
              {t("georef.action.apply")}
            </button>
          </div>
        </div>
      )}
      <div className="visually-hidden" role="status">
        {summary}
      </div>
    </form>
  );
}
