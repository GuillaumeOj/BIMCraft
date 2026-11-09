import { useCallback, useEffect, useRef, useState } from "react";
import {
  type EntityId,
  getMaterial,
  listMaterials,
  type MaterialCategory,
  type MaterialDetail,
  type MaterialSummary,
  useApi,
} from "../api";
import { useDebounced } from "./useDebounced";

export type Loaded<T> = { status: "loading" } | { status: "ready"; value: T } | { status: "error" };

/**
 * The project list (filtered by `search` and `category`) and the detail of the selected material,
 * read again after every change of the model. An older answer never replaces a newer one.
 */
export function useMaterialsData(
  search: string,
  category: MaterialCategory | null,
  selected: EntityId | null,
  onDetailError: (error: unknown) => void,
) {
  const api = useApi();
  const query = useDebounced(search, 200);
  const [list, setList] = useState<Loaded<MaterialSummary[]>>({ status: "loading" });
  const [detail, setDetail] = useState<MaterialDetail | null>(null);
  const listRequest = useRef(0);
  const detailRequest = useRef(0);

  const reloadList = useCallback(() => {
    listRequest.current += 1;
    const mine = listRequest.current;
    listMaterials(api, { search: query, category }).then(
      (items) => mine === listRequest.current && setList({ status: "ready", value: items }),
      () => mine === listRequest.current && setList({ status: "error" }),
    );
  }, [api, query, category]);

  const reloadDetail = useCallback(() => {
    detailRequest.current += 1;
    const mine = detailRequest.current;
    if (selected === null) return setDetail(null);
    getMaterial(api, selected).then(
      (material) => mine === detailRequest.current && setDetail(material),
      (error: unknown) => mine === detailRequest.current && onDetailError(error),
    );
  }, [api, selected, onDetailError]);

  useEffect(reloadList, [reloadList]);
  useEffect(reloadDetail, [reloadDetail]);

  useEffect(() => {
    const unsubscribe = api
      .onChange(() => {
        reloadList();
        reloadDetail();
      })
      .catch(() => undefined);
    return () => {
      void unsubscribe.then((stop) => stop?.());
    };
  }, [api, reloadList, reloadDetail]);

  const reload = useCallback(() => {
    reloadList();
    reloadDetail();
  }, [reloadList, reloadDetail]);

  return { query, list, detail, reload };
}
