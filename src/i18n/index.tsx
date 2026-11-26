import React, { createContext, useContext } from 'react';
import { en, type TranslationKeys } from './en';
import { id } from './id';
import type { LanguageCode } from '../types/envstack';

interface I18nContextValue {
  lang: LanguageCode;
  t: TranslationKeys;
  format: (template: string, vars?: Record<string, string | number>) => string;
}

const dictionaries: Record<LanguageCode, TranslationKeys> = { en, id };

const I18nContext = createContext<I18nContextValue>({
  lang: 'en',
  t: en,
  format: (tpl) => tpl,
});

export function I18nProvider({
  lang,
  children,
}: {
  lang: LanguageCode;
  children: React.ReactNode;
}) {
  const t = dictionaries[lang] ?? en;

  const format = (template: string, vars?: Record<string, string | number>) => {
    if (!vars) return template;
    return Object.entries(vars).reduce(
      (acc, [k, v]) => acc.replaceAll(`{${k}}`, String(v)),
      template,
    );
  };

  return (
    <I18nContext.Provider value={{ lang, t, format }}>
      {children}
    </I18nContext.Provider>
  );
}

export function useI18n() {
  return useContext(I18nContext);
}
