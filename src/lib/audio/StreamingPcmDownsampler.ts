/**
 * StreamingPcmDownsampler
 * 
 * High-performance continuous box-filter downsampler converting
 * arbitrary WebAudio sample rates (44.1kHz, 48kHz, 96kHz Float32)
 * into standard 16kHz Int16 Mono PCM for STT engines (Deepgram / Whisper).
 * 
 * Mathematical Formulation:
 * S_out[m] = (1 / Δ) * ∫ S_in(t) dt over [m * Δ, (m + 1) * Δ]
 */

export interface DownsamplerConfig {
  inputSampleRate: number;
  outputSampleRate?: number; // default: 16000
}

export class StreamingPcmDownsampler {
  private readonly inputSampleRate: number;
  private readonly outputSampleRate: number;
  private readonly ratio: number; // inputSampleRate / outputSampleRate
  private bufferRemainder: Float32Array = new Float32Array(0);

  constructor(config: DownsamplerConfig) {
    this.inputSampleRate = Math.max(8000, config.inputSampleRate);
    this.outputSampleRate = config.outputSampleRate ?? 16000;
    this.ratio = this.inputSampleRate / this.outputSampleRate;
  }

  /**
   * Process a chunk of Float32 audio samples (-1.0 to 1.0) and emit Int16 PCM.
   */
  public process(inputChunk: Float32Array): Int16Array {
    if (inputChunk.length === 0) {
      return new Int16Array(0);
    }

    // Combine any leftover samples from previous iteration
    const totalLength = this.bufferRemainder.length + inputChunk.length;
    const combined = new Float32Array(totalLength);
    combined.set(this.bufferRemainder, 0);
    combined.set(inputChunk, this.bufferRemainder.length);

    const outputSamplesCount = Math.floor(totalLength / this.ratio);
    if (outputSamplesCount <= 0) {
      this.bufferRemainder = combined;
      return new Int16Array(0);
    }

    const output = new Int16Array(outputSamplesCount);

    // Continuous box filter downsampling
    for (let i = 0; i < outputSamplesCount; i++) {
      const startPos = i * this.ratio;
      const endPos = (i + 1) * this.ratio;

      let sum = 0;
      const startIdx = Math.floor(startPos);
      const endIdx = Math.min(Math.ceil(endPos), combined.length);

      for (let j = startIdx; j < endIdx; j++) {
        const segStart = Math.max(startPos, j);
        const segEnd = Math.min(endPos, j + 1);
        const weight = Math.max(0, segEnd - segStart);
        sum += (combined[j] || 0) * weight;
      }

      const average = sum / this.ratio;
      // Clamp to -1.0 .. 1.0 and scale to Int16 (-32768 .. 32767)
      const clamped = Math.max(-1.0, Math.min(1.0, average));
      output[i] = clamped < 0 ? Math.round(clamped * 32768) : Math.round(clamped * 32767);
    }

    // Save remaining fraction for next call
    const consumedInputLength = Math.floor(outputSamplesCount * this.ratio);
    if (consumedInputLength < combined.length) {
      this.bufferRemainder = combined.slice(consumedInputLength);
    } else {
      this.bufferRemainder = new Float32Array(0);
    }

    return output;
  }

  /**
   * Reset internal buffer state.
   */
  public reset(): void {
    this.bufferRemainder = new Float32Array(0);
  }
}
