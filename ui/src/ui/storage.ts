// Browser storage can be missing or throw (private windows, blocked site data, a full quota).
// A value whose write failed is kept in memory and wins over what storage holds, so a choice
// still lasts for the session whichever way storage fails; nothing here ever throws.

type Store = "local" | "session";

const area = (store: Store): Storage => (store === "local" ? localStorage : sessionStorage);

// Values whose last write failed; `null` remembers a removal that failed.
const memory: Record<Store, Map<string, string | null>> = { local: new Map(), session: new Map() };

/** The value stored under `key`, or null when there is none. */
export function readStored(key: string, store: Store = "local"): string | null {
  const kept = memory[store];
  if (kept.has(key)) return kept.get(key) ?? null;
  try {
    return area(store).getItem(key);
  } catch {
    return null;
  }
}

/** Stores `value` under `key`, or removes the key for null; in memory when storage fails. */
export function writeStored(key: string, value: string | null, store: Store = "local"): void {
  try {
    if (value === null) area(store).removeItem(key);
    else area(store).setItem(key, value);
    memory[store].delete(key);
  } catch {
    memory[store].set(key, value);
  }
}

/** Forgets the values kept in memory, as clearing storage does; for tests. */
export function forgetStoredInMemory(): void {
  memory.local.clear();
  memory.session.clear();
}
