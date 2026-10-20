import { useI18n } from "./i18n/I18nProvider";
import { LanguageSelect } from "./i18n/LanguageSelect";

/** Root component of the BIMCraft interface. */
export function App() {
  const { t } = useI18n();
  return (
    <>
      <header>
        <LanguageSelect />
      </header>
      <main>
        <h1>{t("app.name")}</h1>
      </main>
    </>
  );
}
