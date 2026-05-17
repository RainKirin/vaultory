import { createContext, useContext } from "react";
import type { Lang } from "../i18n/translations";

interface I18nContextType {
  lang: Lang;
  t: (key: string) => string;
  setLang: (lang: Lang) => void;
}

export const I18nContext = createContext<I18nContextType>({
  lang: "zh",
  t: (key: string) => key,
  setLang: () => {},
});

export function useI18n() {
  return useContext(I18nContext);
}
