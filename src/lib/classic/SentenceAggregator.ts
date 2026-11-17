export interface SentenceAggregatorConfig {
  silenceThresholdMs?: number; // Flush after silence
  maxSentenceChars?: number;   // Max length before soft-cut
}

export interface AggregatedSentence {
  text: string;
  isFinal: boolean;
  timestamp: number;
}

/**
 * SentenceAggregator buffers streaming speech-to-text tokens/deltas
 * and splits them into semantic, syntactically complete sentences
 * for downstream LLM translation and TTS synthesis.
 */
export class SentenceAggregator {
  private buffer: string = '';
  private lastAppendTime: number = Date.now();
  private silenceThresholdMs: number;
  private maxSentenceChars: number;

  // Regex matching sentence boundaries across Latin, Cyrillic, CJK, and Arabic scripts
  private static readonly BOUNDARY_REGEX = /([.!?。！？\n]+|\.{3})\s*/g;
  private static readonly CLAUSE_BREAK_REGEX = /([,;:，；、])\s*/g;

  constructor(config: SentenceAggregatorConfig = {}) {
    this.silenceThresholdMs = config.silenceThresholdMs ?? 750;
    this.maxSentenceChars = config.maxSentenceChars ?? 140;
  }

  /**
   * Append incoming STT delta text
   */
  public pushDelta(delta: string): string[] {
    if (!delta) return [];

    this.buffer += delta;
    this.lastAppendTime = Date.now();

    return this.extractCompletedSentences();
  }

  /**
   * Periodically check if silence threshold has elapsed to flush remaining text
   */
  public checkSilenceFlush(): string | null {
    if (!this.buffer.trim()) {
      return null;
    }

    const elapsed = Date.now() - this.lastAppendTime;
    if (elapsed >= this.silenceThresholdMs) {
      const sentence = this.buffer.trim();
      this.buffer = '';
      return sentence;
    }
    return null;
  }

  /**
   * Force flush all buffered text
   */
  public flush(): string | null {
    const text = this.buffer.trim();
    this.buffer = '';
    return text.length > 0 ? text : null;
  }

  /**
   * Inspect current interim buffer
   */
  public getInterimBuffer(): string {
    return this.buffer;
  }

  /**
   * Extract complete sentences terminated by punctuation or max length
   */
  private extractCompletedSentences(): string[] {
    const completed: string[] = [];
    SentenceAggregator.BOUNDARY_REGEX.lastIndex = 0;

    let match: RegExpExecArray | null;
    let lastCutIndex = 0;

    while ((match = SentenceAggregator.BOUNDARY_REGEX.exec(this.buffer)) !== null) {
      const sentenceEnd = match.index + match[0].length;
      const sentence = this.buffer.substring(lastCutIndex, sentenceEnd).trim();
      if (sentence.length > 0) {
        completed.push(sentence);
      }
      lastCutIndex = sentenceEnd;
    }

    if (lastCutIndex > 0) {
      this.buffer = this.buffer.substring(lastCutIndex);
    }

    // If buffer exceeds max chars without punctuation, attempt soft clause break
    if (this.buffer.length >= this.maxSentenceChars) {
      const clauseCut = this.findClauseCut(this.buffer);
      if (clauseCut > 0) {
        const sentence = this.buffer.substring(0, clauseCut).trim();
        completed.push(sentence);
        this.buffer = this.buffer.substring(clauseCut).trimStart();
      }
    }

    return completed;
  }

  private findClauseCut(text: string): number {
    SentenceAggregator.CLAUSE_BREAK_REGEX.lastIndex = 0;
    let match: RegExpExecArray | null;
    let bestCut = -1;

    while ((match = SentenceAggregator.CLAUSE_BREAK_REGEX.exec(text)) !== null) {
      if (match.index >= 40 && match.index <= this.maxSentenceChars) {
        bestCut = match.index + match[0].length;
      }
    }

    // If no comma found, fallback to nearest space
    if (bestCut === -1) {
      const lastSpace = text.lastIndexOf(' ', this.maxSentenceChars);
      if (lastSpace > 40) {
        bestCut = lastSpace + 1;
      }
    }

    return bestCut;
  }
}
