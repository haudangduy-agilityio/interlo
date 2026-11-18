import {
  createContext,
  createElement,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import type { UiLanguage } from "../ipc/types";
import { en, type MessageKey } from "./messages.en";
import { vi } from "./messages.vi";

export type { MessageKey } from "./messages.en";

export type MessageParams = Readonly<Record<string, string | number>>;

const DICTIONARIES: Record<UiLanguage, Readonly<Record<MessageKey, string>>> = { en, vi };

/** The locale tag that date and number formatting follows for each interface language. */
const LOCALES: Record<UiLanguage, string> = { en: "en-US", vi: "vi-VN" };
export const localeFor = (language: UiLanguage): string => LOCALES[language];

function fill(text: string, params: MessageParams | undefined): string {
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in params ? String(params[name]) : whole,
  );
}

/**
 * The text for `key` in `language`. A key that is missing or empty in that language gives the
 * English text, so a half-translated dictionary never shows a blank label.
 */
export function translate(language: UiLanguage, key: MessageKey, params?: MessageParams): string {
  const own = DICTIONARIES[language][key];
  return fill(own !== undefined && own !== "" ? own : en[key], params);
}

/** English text, for code that runs outside a screen (the IPC client, error mapping). */
export const t = (key: MessageKey, params?: MessageParams): string => translate("en", key, params);

export interface LanguageApi {
  language: UiLanguage;
  setLanguage(language: UiLanguage): void;
}

// Without a provider the app speaks English, so a view can render on its own in a test.
const LanguageContext = createContext<LanguageApi>({ language: "en", setLanguage: () => {} });

/**
 * Holds the interface language and writes it to `<html lang>`. The settings value feeds it:
 * `SettingsLanguageSync` sets it when the app starts and the General tab sets it on a change.
 */
export function LanguageProvider({
  initial = "en",
  children,
}: {
  initial?: UiLanguage;
  children: ReactNode;
}) {
  const [language, setLanguage] = useState<UiLanguage>(initial);
  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);
  const value = useMemo(() => ({ language, setLanguage }), [language]);
  return createElement(LanguageContext.Provider, { value }, children);
}

export function useLanguage(): LanguageApi {
  return useContext(LanguageContext);
}

/** `t` bound to the current interface language. */
export function useT(): (key: MessageKey, params?: MessageParams) => string {
  const { language } = useLanguage();
  return useCallback((key, params) => translate(language, key, params), [language]);
}
