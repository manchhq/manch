---
governs:
  paths: []
  shapes: [product]
  verify: ask
---

# Product intent

This document records intent from the existing README. Its claims are
**unverifiable-from-source**; they are not implementation guarantees.
Source: [README.md](../README.md).

## Purpose and audience

Manch supplies reusable, domain-free Rust plumbing for products that host AI
agents. Consumers implement and register extension traits in their own
repositories, spending their effort on domain logic. An optional server and a
Tauri reference application consume the libraries. Source:
[README.md: What Manch is and Design stance](../README.md).

## Out of scope

Domain logic, model resale and billing, and requiring every consumer to run a
server are deliberate exclusions. Source:
[README.md: What Manch deliberately excludes](../README.md).

## Deferred

README.md describes Bedrock and Vertex support as demand-driven future work and
core API stabilization as unfinished. Its milestone text is a stated roadmap,
not evidence of current implementation status. Source:
[README.md: Provider roadmap and Status & first milestone](../README.md).

Additional priorities or deadlines: not recorded.
