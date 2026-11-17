import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import { readStored, writeStored } from "../ui/storage";
import {
  asLanguage,
  type Language,
  type MessageKey,
  type MessageParams,
  resolveLanguage,
  translate,
} from "./i18n";

/** Where the manual language choice is kept between sessions. */
export const OVERRIDE_STORAGE_KEY = "fabrilo.language";

export type I18n = {
  language: Language;
  /** The manual choice, or `null` when the language follows the system. */
  override: Language | null;
  /** Sets a manual language, or `null` to follow the system again. */
  setLanguage: (override: Language | null) => void;
  t: (key: MessageKey, params?: MessageParams) => string;
};

const I18nContext = createContext<I18n | null>(null);

// Without storage, the choice lasts only for this session.
const readOverride = (): Language | null => asLanguage(readStored(OVERRIDE_STORAGE_KEY));
const writeOverride = (override: Language | null) => writeStored(OVERRIDE_STORAGE_KEY, override);

/** Provides the interface language: the manual override if set, else the system language. */
export function I18nProvider({ children }: { children: ReactNode }) {
  const [override, setOverride] = useState(readOverride);
  const [systemLanguages, setSystemLanguages] = useState(() => navigator.languages);
  const language = resolveLanguage(override, systemLanguages);

  useEffect(() => {
    const onChange = () => setSystemLanguages(navigator.languages);
    window.addEventListener("languagechange", onChange);
    return () => window.removeEventListener("languagechange", onChange);
  }, []);

  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);

  const setLanguage = useCallback((next: Language | null) => {
    writeOverride(next);
    setOverride(next);
  }, []);

  const value = useMemo<I18n>(
    () => ({
      language,
      override,
      setLanguage,
      t: (key, params) => translate(language, key, params),
    }),
    [language, override, setLanguage],
  );
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

/** The current language, its translator `t` and `setLanguage`. */
export function useI18n(): I18n {
  const i18n = useContext(I18nContext);
  if (i18n === null) throw new Error("useI18n must be used inside <I18nProvider>");
  return i18n;
}
