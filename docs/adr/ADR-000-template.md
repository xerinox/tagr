# ADR-000: Template

> Copy to `ADR-NNN-short-title.md`, increment `NNN`, delete this blockquote and any
> guidance in _italics_. Keep the section order.

- **Status:** Proposed
- **Date:** YYYY-MM-DD
- **Slice:** _e.g. Slice 2 — depth in the core_
- **Supersedes:** _ADR-NNN, or_ —
- **Superseded by:** —

## Context

_What is true right now that forces a choice? Constraints, not preferences: what the
code already does, what the layer model requires, what the CLI-first philosophy
implies, what performance or compatibility demands. Someone who has never seen the
codebase should be able to see why a decision was unavoidable here._

_If you cannot write this section without describing the solution, you have not found
the real question yet._

## Options considered

_At least two, described fairly. The rejected options must be ones you would have been
willing to ship. If the alternatives are strawmen, this ADR is a rationalisation._

### Option A — _name_

_What it is._

- **For:**
- **Against:**

### Option B — _name_

_What it is._

- **For:**
- **Against:**

## Decision

_One paragraph, in the active voice: "We store tags in…", not "It was decided that…".
State the choice and the single reason that actually settled it — not every reason
that supports it._

## Consequences

_What becomes true as a result. Include the bad parts; an ADR with no negative
consequences is not describing a real tradeoff._

- **Enables:**
- **Costs:**
- **Constrains:** _what future decisions this now forecloses_
- **Revisit if:** _the concrete condition that would make this decision wrong_

## Notes

_Optional. Benchmarks, links, prior art, the legacy implementation this replaces and
what it got right. Delete if empty._

---

<!--
House rules (see docs/architecture/handcrafted-rewrite-guide.md):

1. Write the ADR BEFORE the code. A record written afterwards rationalises; one
   written beforehand decides.
2. If you cannot name a rejected option, you have not made a decision — you have
   copied one. Stop and find the alternative.
3. Supersede, never edit. When a decision changes, set this file's status to
   `Superseded by ADR-NNN` and write a new file. The trail of changed minds is the
   point; overwriting destroys it.
4. No LLM-authored ADRs. Rubber-duck the tradeoffs if useful, then write it yourself.

Status values: Proposed | Accepted | Superseded | Rejected
-->
