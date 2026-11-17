/**
 * LinearPcmResampler
 * 
 * High-performance linear interpolation resampler converting
 * 16kHz Int16 PCM (Deepgram / Audio standard) to 24kHz Int16 PCM
 * (OpenAI Realtime WebSocket API specification), and vice-versa.
 * 
 * Mathematical Formulation:
 * S_out[n] = (1 - α) * S_in[k] + α * S_in[k + 1]
 * where k = floor(n * r_in / r_out) and α = (n * r_in / r_out) - k
 */

export class LinearPcmResampler {
  /**
   * Resample Int16 PCM from 16kHz to 24kHz.
   */
  public static resample16kTo24k(input: Int16Array): Int16Array {
    if (input.length === 0) return new Int16Array(0);

    const ratio = 16000 / 24000; // 2/3
    const outputLength = Math.round(input.length / ratio);
    const output = new Int16Array(outputLength);

    for (let i = 0; i < outputLength; i++) {
      const position = i * ratio;
      const index = Math.floor(position);
      const fraction = position - index;

      const sampleA = input[index] || 0;
      const sampleB = index + 1 < input.length ? input[index + 1] : sampleA;

      const interpolated = sampleA + (sampleB - sampleA) * fraction;
      output[i] = Math.max(-32768, Math.min(32767, Math.round(interpolated)));
    }

    return output;
  }

  /**
   * Resample Int16 PCM from 24kHz to 16kHz.
   */
  public static resample24kTo16k(input: Int16Array): Int16Array {
    if (input.length === 0) return new Int16Array(0);

    const ratio = 24000 / 16000; // 1.5
    const outputLength = Math.round(input.length / ratio);
    const output = new Int16Array(outputLength);

    for (let i = 0; i < outputLength; i++) {
      const position = i * ratio;
      const index = Math.floor(position);
      const fraction = position - index;

      const sampleA = input[index] || 0;
      const sampleB = index + 1 < input.length ? input[index + 1] : sampleA;

      const interpolated = sampleA + (sampleB - sampleA) * fraction;
      output[i] = Math.max(-32768, Math.min(32767, Math.round(interpolated)));
    }

    return output;
  }
}
