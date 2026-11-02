# ADR 0003: Path-based rules as data, with container and leaf semantics

**Status:** accepted

## Context

Classification must be explainable, testable and easy for contributors to extend.

## Decision

Classification is a static, ordered rule table (`classifier/rules.rs`). Every rule carries a label, an explanation and a confidence. Rules are _containers_ (more specific rules may match inside) or _leaves_ (they claim their subtree). Size is never an input.

## Consequences

- Every classification can show the rule that produced it.
- Adding a location is a data change plus a test.
- Heuristics that need content inspection (e.g. "this `node_modules` belongs to an abandoned project") belong in the recommendation engine, not the classifier.
