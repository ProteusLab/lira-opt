# lira-arch-opt

`lira-arch-opt` generalizes an arch, optimizes every snippet and instruction
independently, then specializes it. Cases are independent and run on a rayon
pool (`-j`). Three budgets decide whether a case finishes, and each has a
defined effect on the output.

## Time

- `--timeout-per-case-soft` (default 1 s): the time a case may take for free.
- `--timeout-per-case-hard` (default 60 s): absolute cap per case.
- `--timeout-global-extra` (default: `soft × number of cases`): shared pool
  charged with each case's time above the soft budget.
- Effective deadline per case: `min(soft + remaining_pool, hard)`.

A case that reaches its deadline is a **soft skip**: its statements are left
untouched, so the output stays valid but unoptimized for that case. When the
pool is exhausted the process logs `global time pool exhausted` and **exits 1
without writing the output file**.

Saturation checks the deadline before every rule, so one slow rule cannot run
a pass past the deadline. Extraction runs under the same deadline;
`thorough` gives its primary MaxSAT stage 90 % of the remaining budget and
then runs the fallback ("hammer"), so long extraction cases tend to cluster
at `0.9 × hard`.

## Memory

Each case builds its own egraph, extraction graph and SMT encoding and then
drops them; the allocator keeps each worker's high-water mark, so resident
memory tracks the largest case a worker has seen, not the number of cases.
Without the caps below this is ~1.5–2.5 GB per active worker.

- `Z3_MEMORY_MB` (default 512; `0` disables): z3 runs as a child process
  with `-memory:<MB>` (virtual memory). On the cap z3 reports out of memory;
  the extractor treats that like a timeout, falls back to the hammer, and
  keeps the best assignment if an earlier stage produced one. If no stage
  produced a result, the case is reported as an error and its statements are
  unchanged. Keep the cap well above z3's construction footprint: a very low
  cap makes z3 die while the formula is being built (the tool catches that
  panic per case, but the case is then unoptimized).
- `LIRA_MAX_ROWS` (default 200 000): sum of rows over all tables, checked
  before every rule. Exceeding it is a soft skip; set `usize::MAX` to
  disable.

## Restrictions and what they preserve

| restriction | checked | on hit | case output |
|---|---|---|---|
| soft/hard deadline | per rule, extraction | skip | original statements |
| global pool | after each case | exit 1 | no output file |
| `LIRA_MAX_ROWS` | per rule | skip | original statements |
| `Z3_MEMORY_MB` | by z3 | fallback, else error | best-effort, else original |

## Sizing

Budget ~1 GB per worker with both caps, ~2.5 GB without them. On a 30 GB
machine with ~12 GB used by the desktop, 12 workers fit with the caps; the
same run without caps is killed by the OOM killer at 8–10 workers.
