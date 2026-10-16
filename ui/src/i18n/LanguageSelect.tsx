import { useI18n } from "./I18nProvider";
import { asLanguage, LANGUAGES } from "./i18n";
import "./LanguageSelect.css";

const SYSTEM = "system";

/** Chooses the interface language, or goes back to following the system (UI-04). */
export function LanguageSelect() {
  const { override, setLanguage, t } = useI18n();
  return (
    <label className="language-select">
      {t("language.label")}
      <select
        value={override ?? SYSTEM}
        // "system" is not a language, so it clears the override.
        onChange={(event) => setLanguage(asLanguage(event.target.value))}
      >
        <option value={SYSTEM}>{t("language.system")}</option>
        {LANGUAGES.map((language) => (
          <option key={language} value={language}>
            {t(`language.${language}`)}
          </option>
        ))}
      </select>
    </label>
  );
}
