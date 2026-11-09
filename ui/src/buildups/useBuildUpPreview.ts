import { useEffect, useState } from "react";
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
  const inputs = layerInputs(draft);
  const question = useDebounced(JSON.stringify([draft.usage, inputs, revision]), DELAY_MS);
  const [preview, setPreview] = useState<BuildUpPreview | null>(null);

  useEffect(() => {
    let current = true;
    const [usage, layers] = JSON.parse(question) as [
      Draft["usage"],
      ReturnType<typeof layerInputs>,
    ];
    if (layers === null) {
      setPreview(null);
      return;
    }
    previewBuildUp(api, usage, layers).then(
      (result) => current && setPreview(result),
      () => current && setPreview(null),
    );
    return () => {
      current = false;
    };
  }, [api, question]);

  return preview;
}
