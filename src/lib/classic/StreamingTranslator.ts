export interface TranslatorConfig {
  sourceLanguage: string;
  targetLanguage: string;
  glossary?: Record<string, string>;
  model?: string;
  temperature?: number;
}

export interface TranslationResult {
  originalText: string;
  translatedText: string;
  durationMs: number;
}

/**
 * Filter against STT vocabulary prompt echo leaks
 */
export function isLikelyVocabularyPromptLeak(text: string, glossaryTerms: string[]): boolean {
  if (!text || glossaryTerms.length === 0) return false;
  const normalized = text.toLowerCase().normalize('NFKC');

  // Check if text is just a concatenation or comma-separated list of glossary terms
  let termMatchCount = 0;
  for (const term of glossaryTerms) {
    if (normalized.includes(term.toLowerCase())) {
      termMatchCount++;
    }
  }

  // If more than 75% of words are from the glossary without actual grammatical verbs, flag as prompt leak
  const wordCount = normalized.split(/\s+/).filter(Boolean).length;
  if (wordCount > 0 && termMatchCount >= 3 && termMatchCount / wordCount > 0.6) {
    return true;
  }
  return false;
}

/**
 * StreamingTranslator handles LLM-based spoken translation with context memory,
 * glossary adaptation, and anti-hallucination guards.
 */
export class StreamingTranslator {
  private config: TranslatorConfig;
  private recentHistory: Array<{ original: string; translated: string }> = [];
  private static readonly MAX_HISTORY_ITEMS = 3;

  constructor(config: TranslatorConfig) {
    this.config = {
      model: 'gpt-4o-mini',
      temperature: 0.1,
      ...config
    };
  }

  public setLanguages(sourceLang: string, targetLang: string) {
    this.config.sourceLanguage = sourceLang;
    this.config.targetLanguage = targetLang;
    this.recentHistory = [];
  }

  public setGlossary(glossary: Record<string, string>) {
    this.config.glossary = glossary;
  }

  /**
   * Generates strict translation system prompt
   */
  public buildSystemPrompt(): string {
    const src = this.config.sourceLanguage;
    const tgt = this.config.targetLanguage;

    let prompt =
      `You are an ultra-accurate live meeting translation engine.\n` +
      `Translate the provided spoken text from ${src} to ${tgt}.\n` +
      `STRICT CONSTRAINTS:\n` +
      `1. Return ONLY the translated sentence.\n` +
      `2. Do NOT add notes, greetings, explanations, or quotes.\n` +
      `3. Preserve spoken tone and intent accurately.\n` +
      `4. If the sentence is already in ${tgt}, return it as is.`;

    if (this.config.glossary && Object.keys(this.config.glossary).length > 0) {
      const terms = Object.entries(this.config.glossary)
        .map(([k, v]) => `"${k}" -> "${v}"`)
        .join(', ');
      prompt += `\nGLOSSARY REQUIREMENT: Strictly use these term translations: ${terms}.`;
    }

    return prompt;
  }

  /**
   * Applies pre-translation glossary substitution or pre-processing
   */
  public preprocess(text: string): string {
    let clean = text.trim();
    if (this.config.glossary) {
      const glossaryKeys = Object.keys(this.config.glossary);
      if (isLikelyVocabularyPromptLeak(clean, glossaryKeys)) {
        return '';
      }
    }
    return clean;
  }

  /**
   * Translates a single sentence with context
   */
  public async translate(
    text: string,
    executePromptFn?: (messages: Array<{ role: string; content: string }>) => Promise<string>
  ): Promise<TranslationResult> {
    const start = Date.now();
    const cleanInput = this.preprocess(text);

    if (!cleanInput) {
      return { originalText: text, translatedText: '', durationMs: 0 };
    }

    // Prepare message history
    const messages: Array<{ role: string; content: string }> = [
      { role: 'system', content: this.buildSystemPrompt() }
    ];

    // Inject recent context
    for (const h of this.recentHistory) {
      messages.push({ role: 'user', content: h.original });
      messages.push({ role: 'assistant', content: h.translated });
    }

    messages.push({ role: 'user', content: cleanInput });

    let translated = '';
    if (executePromptFn) {
      translated = await executePromptFn(messages);
    } else {
      // Clean fallback if no executor provided
      translated = `[Translated to ${this.config.targetLanguage}]: ${cleanInput}`;
    }

    const durationMs = Date.now() - start;

    // Save to history
    this.recentHistory.push({ original: cleanInput, translated });
    if (this.recentHistory.length > StreamingTranslator.MAX_HISTORY_ITEMS) {
      this.recentHistory.shift();
    }

    return {
      originalText: cleanInput,
      translatedText: translated,
      durationMs
    };
  }

  public clearHistory(): void {
    this.recentHistory = [];
  }
}
