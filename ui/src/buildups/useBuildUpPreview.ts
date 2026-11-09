import { useEffect, useMemo, useRef, useState } from "react";
import { type BuildUpPreview, previewBuildUp, useApi } from "../api";
import { useDebounced } from "../materials/useDebounced";
import { type Draft, layerInputs } from "./draft";

const DELAY_MS = 250;

/**
 * The core's check of the draft (totals, thermal resistance, every error), asked for after the
 * draft has stopped changing. Null until the first answer, while a layer has no material (the
 * core cannot be asked) and when the question fails.
 */
export function useBuildUpPreview(draft: Draft, revision: number): BuildUpPreview | null {
  const api = useApi();
  const { usage, layers } = draft;
  const inputs = useMemo(() => layerInputs({ ...draft, layers }), [draft, layers]);
  // A name or description edit leaves the question as it was: no new call.
  const question = useDebounced(
    useMemo(() => JSON.stringify([usage, inputs, revision]), [usage, inputs, revision]),
    DELAY_MS,
  );
  const latest = useRef({ usage, inputs });
  latest.current = { usage, inputs };
  const [preview, setPreview] = useState<BuildUpPreview | null>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `question` is the debounced trigger; the data is read from `latest`.
  useEffect(() => {
    let current = true;
    const { usage: asked, inputs: toAsk } = latest.current;
    if (toAsk === null) {
      setPreview(null);
      return;
    }
    previewBuildUp(api, asked, toAsk).then(
      (result) => current && setPreview(result),
      () => current && setPreview(null),
    );
    return () => {
      current = false;
    };
  }, [api, question]);

  return preview;
}
