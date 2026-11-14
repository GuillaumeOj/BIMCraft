// Browser storage can be missing or throw (private windows, blocked site data). Values are then
// kept in memory, so a choice still lasts for the session; nothing here ever throws.

type Store = "local" | "session";

const area = (store: Store): Storage => (store === "local" ? localStorage : sessionStorage);

// Consulted only while storage cannot be used.
const memory: Record<Store, Map<string, string>> = { local: new Map(), session: new Map() };

/** The value stored under `key`, or null when there is none. */
export function readStored(key: string, store: Store = "local"): string | null {
  try {
    return area(store).getItem(key);
  } catch {
    return memory[store].get(key) ?? null;
  }
}

/** Stores `value` under `key`, or removes the key for null; in memory when storage cannot be used. */
export function writeStored(key: string, value: string | null, store: Store = "local"): void {
  try {
    if (value === null) area(store).removeItem(key);
    else area(store).setItem(key, value);
  } catch {
    if (value === null) memory[store].delete(key);
    else memory[store].set(key, value);
  }
}
