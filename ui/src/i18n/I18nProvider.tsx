import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import {
  asLanguage,
  type Language,
  type MessageKey,
  type MessageParams,
  resolveLanguage,
  translate,
} from "./i18n";

/** Where the manual language choice is kept between sessions. */
export const OVERRIDE_STORAGE_KEY = "bimcraft.language";

export type I18n = {
  language: Language;
  /** The manual choice, or `null` when the language follows the system. */
  override: Language | null;
  /** Sets a manual language, or `null` to follow the system again. */
  setLanguage: (override: Language | null) => void;
  t: (key: MessageKey, params?: MessageParams) => string;
};

const I18nContext = createContext<I18n | null>(null);

// Storage can be missing or throw (private windows, blocked site data); the choice then lasts
// only for this session.
function readOverride(): Language | null {
  try {
    return asLanguage(localStorage.getItem(OVERRIDE_STORAGE_KEY));
  } catch {
    return null;
  }
}

function writeOverride(override: Language | null) {
  try {
    if (override === null) localStorage.removeItem(OVERRIDE_STORAGE_KEY);
    else localStorage.setItem(OVERRIDE_STORAGE_KEY, override);
  } catch {
    // Not remembered; see readOverride.
  }
}

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
