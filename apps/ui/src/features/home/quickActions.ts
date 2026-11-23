import type { MessageKey } from "../../shared/i18n";

/** The three quick actions of the Today screen, with their fixed default keys (D-55). */
export const QUICK_ACTIONS: readonly { id: string; labelKey: MessageKey; keys: string }[] = [
  { id: "translate", labelKey: "home.quickTranslate", keys: "Ctrl+Option+T" },
  { id: "grammar", labelKey: "home.fixGrammar", keys: "Ctrl+Option+G" },
  { id: "note", labelKey: "home.saveNote", keys: "Ctrl+Option+N" },
];
