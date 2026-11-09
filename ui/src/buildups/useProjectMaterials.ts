import { useCallback, useEffect, useRef, useState } from "react";
import { listMaterials, type MaterialSummary, useApi } from "../api";

/** The project's materials, read again after every change of the model. */
export function useProjectMaterials() {
  const api = useApi();
  const [materials, setMaterials] = useState<MaterialSummary[]>([]);
  // Bumped by every read, so that what depends on a material's values (the totals) asks again.
  const [revision, setRevision] = useState(0);
  const latest = useRef(0);

  const reload = useCallback(async () => {
    latest.current += 1;
    const mine = latest.current;
    try {
      const items = await listMaterials(api);
      if (mine === latest.current) {
        setMaterials(items);
        setRevision((current) => current + 1);
      }
      return items;
    } catch {
      // The picker keeps what it had; the editor still works with those.
      return null;
    }
  }, [api]);

  useEffect(() => {
    void reload();
    const unsubscribe = api.onChange(() => void reload()).catch(() => undefined);
    return () => {
      void unsubscribe.then((stop) => stop?.());
    };
  }, [api, reload]);

  return { materials, revision, reload };
}
