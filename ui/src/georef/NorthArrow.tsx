import { useI18n } from "../i18n/I18nProvider";
import { ANGLE_DECIMALS, formatSigned } from "./units";

type NorthArrowProps = {
  /** Angles in degrees, counter-clockwise from the project's y axis; null before Apply. */
  gridNorth: number | null;
  trueNorth: number | null;
};

const SIZE = 64;
const CENTRE = SIZE / 2;

/** A line from the centre, turned `degrees` counter-clockwise from straight up. */
function Needle({ degrees, className }: { degrees: number; className: string }) {
  return (
    <line
      className={className}
      x1={CENTRE}
      y1={CENTRE}
      x2={CENTRE}
      y2={8}
      transform={`rotate(${-degrees} ${CENTRE} ${CENTRE})`}
    />
  );
}

/** Project north, grid north and true north, drawn and written out with their angles. */
export function NorthArrow({ gridNorth, trueNorth }: NorthArrowProps) {
  const { t, language } = useI18n();
  const angle = (value: number | null) =>
    value === null ? "–" : `${formatSigned(value, ANGLE_DECIMALS, language)}°`;
  const rows = [
    ["north-project", "georef.north.project", 0],
    ["north-grid", "georef.north.grid", gridNorth],
    ["north-true", "georef.north.true", trueNorth],
  ] as const;

  return (
    <div className="north">
      <svg
        width={SIZE}
        height={SIZE}
        viewBox={`0 0 ${SIZE} ${SIZE}`}
        role="img"
        aria-label={t("georef.north.diagram")}
      >
        <circle cx={CENTRE} cy={CENTRE} r={30} className="north-dial" />
        <Needle degrees={0} className="north-project" />
        {gridNorth !== null && <Needle degrees={gridNorth} className="north-grid" />}
        {trueNorth !== null && <Needle degrees={trueNorth} className="north-true" />}
      </svg>
      <dl className="north-legend">
        {rows.map(([className, key, value]) => (
          <div key={className} data-emphasis={className === "north-true"}>
            <dt>
              <svg width="20" height="8" viewBox="0 0 20 8" aria-hidden="true">
                <line x1="0" y1="4" x2="20" y2="4" className={className} />
              </svg>
              {t(key)}
            </dt>
            <dd>{angle(value)}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}
