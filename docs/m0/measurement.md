# M1 gate: measurement procedure

How the owner measures the latency gate at the end of M1 (D-33, D-50).

## Definitions (D-50)

- **Unusable recognition** (M0-05 stop rule): word error rate (WER) above 20
  percent on the owner's Vietnamese reading list. 20 percent is a starting
  value. Change it after the first real M0-05 numbers, with a Decision Log row.
- **End of sentence**: the time at which the voice activity detector reports end
  of speech.
- **First sound**: the time at which the first sample of the English voice is
  written to the 16-channel virtual device.
- **One session clock**: the measurement harness (M1-26) logs both times on the
  same session clock. End to end is first sound minus end of sentence.
- **Scope of the gate**: the outgoing chain only (Vietnamese to English) at the
  end of M1. The incoming chain is re-measured at M2-16 under real load, and by
  the repeatable task M0-23 after any fallback step.
- **Voice**: the owner's own voice only. Other accent groups (H-04) are
  measured before M4.
- **Targets**: p95 under 2.0 s and p50 goal under 1.0 s (D-33).

## Reading list

The sentences the owner reads live in `docs/m0/reading-list.md`. That file is
written in task M1-40. Until it exists, no WER or gate run can start.

## Where numbers go

Record each number in a file in `docs/m0/` first. A number goes into the PRD
only after its file exists here (see `README.md` in this folder).

## Power source

Re-check the power source before each long run and note it in that run's
record. Read it as in `baseline.md`: `pmset -g batt` and `pmset -g`. Record AC
power or battery, and whether Low Power Mode is on.

## Run the harness

TODO until M1-26: the harness command. The harness does not exist yet, so no
command is written here.
