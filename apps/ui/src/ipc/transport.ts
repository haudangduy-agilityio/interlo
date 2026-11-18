/** The only thing the client needs from the shell. Swap it to test or to run in a plain browser. */
export interface Transport {
  invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  /** Returns a function that stops listening. */
  listen(event: string, cb: (payload: unknown) => void): () => void;
}
