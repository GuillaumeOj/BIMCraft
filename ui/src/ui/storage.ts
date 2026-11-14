// Browser storage can be missing or throw (private windows, blocked site data). Preferences then
// fall back to their defaults and last only for the session; nothing here ever throws.

type Store = "local" | "session";

const area = (store: Store): Storage => (store === "local" ? localStorage : sessionStorage);

/** The value stored under `key`, or null when there is none or storage cannot be used. */
export function readStored(key: string, store: Store = "local"): string | null {
  try {
    return area(store).getItem(key);
  } catch {
    return null;
  }
}

/** Stores `value` under `key`, or removes the key for null; quietly does nothing without storage. */
export function writeStored(key: string, value: string | null, store: Store = "local"): void {
  try {
    if (value === null) area(store).removeItem(key);
    else area(store).setItem(key, value);
  } catch {
    // Not remembered: see readStored.
  }
}
