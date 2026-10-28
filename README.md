# lira-opt

Term rewriting system and optimization passes for the LIRA data-flow IR.

## Layout

- `lira_trs`: builds a DFG from `lira::StatementSeq` and runs equality saturation rules over it.
- `lira_opt`: optimization transformations on top of `lira_trs` (standard library).
- `lira_ext`: shims for LIRA API that these crates need but upstream does not provide.
  They can be seen as suggestions for upstream LIRA.

## Environment

`z3` must be on `PATH` for the extraction-based tests.
