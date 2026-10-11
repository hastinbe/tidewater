# Clean-room rules

Tidewater reimplements a game engine whose original code belongs to someone else. These rules keep everything in this
repository our own work, and keep a record that shows it. They apply to every contributor, human or AI agent.

## The two sides

Work is split between two sides that never mix:

- **Research** studies the original program (disassembly, debugger traces, recordings of the running game) and writes
  **specifications**. Research happens outside this repository and its material never enters it.
- **Implementation** writes this repository's code, using only the sources allowed below. Nobody who writes code here
  works from the original program's code, decompiled or otherwise.

A person or agent session belongs to one side. An implementation session that has seen disassembly of the original
program stops writing code for the area it saw.

## Allowed sources for implementation

1. **Specifications** in `docs/specs/`, which arrive only through the review described below.
2. **Public documentation** of the formats and behaviour: community wikis, published format write-ups (for example
   the PUD specification), articles. Cite the source in the code or the spec that uses it.
3. **Open-source projects** under licenses compatible with MPL-2.0, read and reused according to their license, with
   attribution in the file that uses them (for example the StormLib-derived decompressors).
4. **The game's data**, inspected at runtime from the user's own installation: archive contents, sprites, maps,
   tables. The data stays on the user's machine; only facts about it (a field's meaning, a count) appear here.
5. **Observed behaviour** of the original game, measured as inputs and outputs (test vectors), without looking at its
   code.

## Never allowed

- Disassembly or decompiler output of the original program, or code translated from it, in any form or language.
- Leaked or otherwise unauthorised source code, or work from anyone who has read it.
- Game files or anything extracted from them: archives, maps, sprites, audio, video, text, screenshots of the
  original game's content. Tests use synthetic data built by our own code.
- Breaking any copy protection.
- Blizzard's trademarks as this project's name, logo or branding. Nominative use ("compatible with *Warcraft II*") is
  fine.

## Specifications

A specification describes **what** happens, never **how the original program's code does it**:

- **Allowed:** prose, tables, formulas, constants, data layouts, state machines, timing, ordering rules, test vectors,
  and names that the data formats themselves define.
- **Not allowed:** pseudocode or code from the original program, its addresses, its function or variable names when
  they come from the program rather than the data, and structure that mirrors its code rather than its behaviour.

Each spec is reviewed against these rules before it is added to `docs/specs/`. The commit that adds it records that
review. Questions from implementation go back to research as requests for a spec, never as "look at what the original
does".

## Record

Git history is the record: when each spec arrived, what code was written after it, and by whom. History is never
rewritten.
