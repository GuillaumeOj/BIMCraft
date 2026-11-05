import { ApiClient, ApiError, type CrsSummary, type Georeference, type Transport } from "../api";

export const LAMBERT93: CrsSummary = {
  epsg: 2154,
  name: "RGF93 v1 / Lambert-93",
  kind: "projected",
  geographic_name: "RGF93 v1",
  area: "France - onshore and offshore, mainland and Corsica",
};
export const CC48: CrsSummary = {
  epsg: 3948,
  name: "RGF93 v1 / CC48",
  kind: "projected",
  geographic_name: "RGF93 v1",
  area: "France - 48°N",
};

export const refusal = (reason: string) => new ApiError(-32000, "refused", { reason });

type Handler = (params: unknown) => unknown;

/**
 * An in-memory core for component tests: it answers the georef.* methods with a simple linear
 * model (no real projection) and pushes `model.changed` after each command. A handler in
 * `override` replaces the built-in one, and may throw an `ApiError`.
 */
export function fakeCore(options: { georeference?: Georeference | null } = {}) {
  let value: Georeference | null = options.georeference ?? null;
  const calls: { method: string; params: unknown }[] = [];
  const listeners = new Set<(message: string) => void>();
  const override: Record<string, Handler> = {};
  const failures: Record<string, Error[]> = {};

  const crs = [LAMBERT93, CC48];
  const rotationOf = () => value?.grid_rotation ?? 0;
  const origin = () => value?.base_point ?? [0, 0, 0];
  const toProjected = ([x, y, z]: number[]) => {
    const [c, s] = [Math.cos(rotationOf()), Math.sin(rotationOf())];
    const [e, n, h] = origin();
    return [
      (e ?? 0) + c * (x ?? 0) - s * (y ?? 0),
      (n ?? 0) + s * (x ?? 0) + c * (y ?? 0),
      (h ?? 0) + (z ?? 0),
    ];
  };
  const toLocal = ([e, n, h]: number[]) => {
    const [c, s] = [Math.cos(rotationOf()), Math.sin(rotationOf())];
    const [e0, n0, h0] = origin();
    const [dx, dy] = [(e ?? 0) - (e0 ?? 0), (n ?? 0) - (n0 ?? 0)];
    return [c * dx + s * dy, -s * dx + c * dy, (h ?? 0) - (h0 ?? 0)];
  };
  const toGeographic = ([e, n, h]: number[]) => [(n ?? 0) / 1e8, (e ?? 0) / 1e8, h ?? 0];
  const fromGeographic = ([lat, lon, h]: number[]) => [(lon ?? 0) * 1e8, (lat ?? 0) * 1e8, h ?? 0];

  const info = () =>
    value === null
      ? { georeference: null }
      : {
          georeference: {
            site: "5".repeat(32),
            value,
            crs: crs.find((entry) => entry.epsg === value?.crs_epsg) ?? LAMBERT93,
            derived: {
              true_north: -0.52,
              convergence: -0.0082,
              point_scale_factor: 0.999925,
              base_point_geographic: toGeographic(value.base_point),
              survey_point_local: value.survey_point === null ? null : toLocal(value.survey_point),
            },
          },
        };

  const builtIn: Record<string, Handler> = {
    "georef.list_crs": () => ({ crs }),
    "georef.get": info,
    "georef.set": (params) => {
      const { site: _site, ...rest } = params as Georeference & { site?: string };
      value = rest;
      return { site: "5".repeat(32) };
    },
    "georef.clear": () => {
      value = null;
      return {};
    },
    "georef.convert": (params) => {
      if (value === null) throw refusal("not_georeferenced");
      const { from, points } = params as { from: string; points: number[][] };
      return {
        points: (points as number[][]).map((point) => {
          const projected =
            from === "local"
              ? toProjected(point)
              : from === "projected"
                ? point
                : fromGeographic(point);
          return { local: toLocal(projected), projected, geographic: toGeographic(projected) };
        }),
      };
    },
  };

  const push = () => {
    const message = JSON.stringify({
      jsonrpc: "2.0",
      method: "model.changed",
      params: { created: [], deleted: [], modified: [] },
    });
    for (const listener of listeners) listener(message);
  };

  const transport: Transport = {
    async send(message) {
      const { id, method, params } = JSON.parse(message);
      calls.push({ method, params });
      try {
        const failure = failures[method]?.shift();
        if (failure) throw failure;
        const result = await (override[method] ?? builtIn[method])?.(params);
        if (method === "georef.set" || method === "georef.clear") queueMicrotask(push);
        return JSON.stringify({ jsonrpc: "2.0", id, result });
      } catch (error) {
        if (!(error instanceof ApiError)) return null;
        const { code, message: text, data } = error;
        return JSON.stringify({ jsonrpc: "2.0", id, error: { code, message: text, data } });
      }
    },
    async listen(onMessage) {
      listeners.add(onMessage);
      return () => listeners.delete(onMessage);
    },
  };

  return {
    client: new ApiClient(transport),
    calls,
    /** Answers `method` with `handler` from now on (it may throw an `ApiError`). */
    override: (method: string, handler: Handler) => {
      override[method] = handler;
    },
    /** Makes the next call of `method` fail with `error`; an `ApiError` is sent as a refusal. */
    failNext: (method: string, error: Error) => {
      failures[method] = [...(failures[method] ?? []), error];
    },
    /** Changes the georeference behind the API's back, as another window would. */
    replace: (next: Georeference | null) => {
      value = next;
    },
    /** What the project's georeference is now. */
    current: () => value,
    /** Sends a `model.changed` notification, as another window or an undo would. */
    notifyChange: push,
  };
}
