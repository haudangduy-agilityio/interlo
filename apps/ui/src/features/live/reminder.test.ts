import { describe, expect, it } from "vitest";
import { REMINDER_INTERVAL_MS, reminderRound } from "./reminder";

describe("reminderRound", () => {
  const start = 1_000_000;

  it("is 0 until 10 minutes have passed", () => {
    expect(REMINDER_INTERVAL_MS).toBe(600_000);
    expect(reminderRound(start, start)).toBe(0);
    expect(reminderRound(start, start + 599_999)).toBe(0);
  });

  it("counts one round for every full 10 minutes", () => {
    expect(reminderRound(start, start + 600_000)).toBe(1);
    expect(reminderRound(start, start + 1_199_999)).toBe(1);
    expect(reminderRound(start, start + 1_200_000)).toBe(2);
  });

  it("is 0 when the clock is behind the start", () => {
    expect(reminderRound(start, start - 5)).toBe(0);
  });
});
