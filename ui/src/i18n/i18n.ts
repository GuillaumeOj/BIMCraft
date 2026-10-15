import en from "./messages/en.json";
import fr from "./messages/fr.json";

/** Languages the interface is translated into (UI-04). English is the reference. */
export const LANGUAGES = ["en", "fr"] as const;
export type Language = (typeof LANGUAGES)[number];
export type Messages = Readonly<Record<string, string>>;
/** Every key of the English file; other languages may lag behind and fall back to English. */
export type MessageKey = keyof typeof en;
export type MessageParams = Readonly<Record<string, string | number>>;

const CATALOGUES: Readonly<Record<Language, Messages>> = { en, fr };

const isLanguage = (value: string): value is Language =>
  (LANGUAGES as readonly string[]).includes(value);

/** Narrows a stored or user-supplied value to a supported language. */
export const asLanguage = (value: string | null): Language | null =>
  value !== null && isLanguage(value) ? value : null;

/** The manual override if any, else the first supported system language, else English. */
export function resolveLanguage(
  override: Language | null,
  systemLanguages: readonly string[],
): Language {
  if (override !== null) return override;
  for (const tag of systemLanguages) {
    const primary = asLanguage(tag.toLowerCase().replace(/-.*/, ""));
    if (primary !== null) return primary;
  }
  return "en";
}

/** Looks a key up in `messages`, then in `fallback`, then returns the key itself, and fills `{param}` placeholders. */
export function translateWith(
  messages: Messages,
  fallback: Messages,
  key: string,
  params: MessageParams = {},
): string {
  const template = messages[key] ?? fallback[key] ?? key;
  return template.replace(/\{(\w+)\}/g, (placeholder, name: string) =>
    String(params[name] ?? placeholder),
  );
}

/** Translates `key` into `language`, falling back to English. */
export const translate = (language: Language, key: MessageKey, params?: MessageParams): string =>
  translateWith(CATALOGUES[language], en, key, params);

/** Keys of `reference` absent from `translation` (missing) and keys only in `translation` (extra). */
export function compareKeys(
  reference: Messages,
  translation: Messages,
): { missing: string[]; extra: string[] } {
  const notIn = (a: Messages, b: Messages) =>
    Object.keys(a)
      .filter((key) => !(key in b))
      .sort();
  return { missing: notIn(reference, translation), extra: notIn(translation, reference) };
}
