# M0-03 bench harness

## What it is

A throwaway tool (D-190) that runs one stage on one WAV file and writes a text log. Every line of
the log is stamped from one session clock, so the times of different events can be subtracted.
Later measurement tasks (M0-04, M0-05, M0-09) add their own stages and read this log.

This harness is not the product's voice activity detector. The one stage it carries, `energy_ref`,
only proves that the harness works: it reports `speech_start` and `speech_end` from the energy of
each 20 ms chunk. The product harness is a later task (M1-26).

The crate has its own `[workspace]`, no dependency, and no product crate imports it. The root gate
(`scripts/check.sh`) does not build it; `bench/M0-03/check.sh` is its gate.

## Build

```bash
cd bench/M0-03
cargo build --release --locked
```

The binary is `bench/M0-03/target/release/m0-03`. Below, `BIN` stands for that path and `T` for an
empty temporary folder outside the repository (`T=$(mktemp -d)`).

## Run

Make the synthetic test file (3.5 s: silence, a tone burst, silence, a tone burst, silence; no
human voice, same bytes every time), then run the stage on it:

```bash
$BIN synth --out $T/a.wav
$BIN run --stage energy_ref --file $T/a.wav --log $T/r1.log
grep -v '^#' $T/r1.log | cut -f2-
```

The last command prints the nine events of the run:

```text
harness	input_open	samples=56000	rate=16000
energy_ref	stage_start
energy_ref	speech_start	sample=8000
energy_ref	speech_end	sample=24000
energy_ref	speech_start	sample=32000
energy_ref	speech_end	sample=48000
harness	input_end	chunks=175
energy_ref	stage_end
harness	session_end
```

Options of `run`:

| Option | Meaning |
|---|---|
| `--stage <name>` | The stage to run. Only `energy_ref` exists. |
| `--file <wav>` | PCM, 16-bit, mono, 16000 Hz. Any other file is refused with an error event. |
| `--log <path>` | The log file. It must not exist yet. Without it the log goes to standard output. |
| `--pace fast\|realtime` | `fast` (default) feeds all chunks at once. `realtime` delivers chunk `i` not before `20 ms * i` after `input_open`, as a live microphone would. The event order is the same in both. |
| `--id <id>` | A sentence ID such as `vi_001`: lower-case letters, digits and `_`, starting with a letter, 1 to 32 characters. It is written in the header only. |

Exit codes: 0 the run finished; 1 the run failed (an `error_*` event is in the log); 2 a usage
error (bad arguments, unknown stage, or a log or WAV path that exists or cannot be created).
The harness never overwrites a file and never writes audio, except `synth` writing the one file
named by `--out`.

## Log format v1

A log is text. Lines that start with `#` are the header. Every other line is one event, with
tab-separated fields:

```text
# interlo-bench-log v1
# wall_start_epoch_ms=<n>
# stage=<name>
# pace=<fast|realtime>
# id=<id>                     (only with --id)
<t_us> <stage> <event> <key>=<n> <key>=<n> ...
```

- `t_us`: microseconds since the start of the session, from a monotonic clock. The clock is read
  once when the session starts (time zero) and once for every event line. Times never go down.
- `stage`: `harness` for the runner, or the stage name.
- Every field value is an unsigned integer. A stage never writes text to the log; the type of the
  sink does not allow it.
- The wall-clock time is only in the header, so a comparison can skip it.
- An error is an event without fields: `error_io`, `error_bad_wav` or `error_bad_format`, then
  `session_end`.

## Compare two runs

Replaying the same file gives the same events in the same order. The time column and the header
differ, so the comparison drops both:

```bash
$BIN run --stage energy_ref --file $T/a.wav --log $T/p1.log
$BIN run --stage energy_ref --file $T/a.wav --log $T/p2.log
diff <(grep -v '^#' $T/p1.log | cut -f2-) <(grep -v '^#' $T/p2.log | cut -f2-)
```

An empty output and exit code 0 mean the two runs have the same event order. A `--pace realtime`
run compares equal to a `--pace fast` run in the same way.

## Add a stage

1. Make a type that implements `Stage` (`src/stage.rs`): `name`, `on_chunk` and `finish`. A stage
   gets 320-sample chunks (20 ms) and a sink. It reports with `sink.emit("event_name", &[("key", number)])`.
   The sink adds the time and the stage name. A stage cannot choose its own time.
2. Add its name to `find` in `src/stage.rs`.
3. Add tests like the ones in `src/energy_ref.rs`: feed chunks to the stage and check the events.

A stage in another bench crate depends on this crate by path (`m0-03 = { path = "../M0-03" }`) and
passes its own stage to `run_session` in `src/run.rs`. A stage that needs text output keeps it in a
channel of its own; the log carries only names, times and numbers (guardrail 6).

## Limits

- One stage and one file per run. No chain of stages, no virtual device, no "first sound" event
  (M0-09, M1-26).
- `--pace realtime` keeps a lower bound: a chunk is never early, but it may be late on a busy Mac.
- WAV input only, one format, no resampling. The reading list is not read; an ID is only copied to
  the header.
- No word error rate and no percentiles. The log is the input of the scripts of M0-04 to M0-09.

## Check

```bash
bench/M0-03/check.sh
```

Runs format, lint, the tests, a names and language check, and a check that no media file is tracked.
The last line is `ALL STEPS PASSED (bench/M0-03)`.
