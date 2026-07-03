# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.1](https://github.com/manchhq/manch/releases/tag/manch-protocol-v0.0.1) - 2026-07-03

### Added

- *(acp)* live-emit events instead of batching after the turn ([#22](https://github.com/manchhq/manch/pull/22))
- *(protocol)* re-export TextContent and add AgentEvent::text_chunk

### Other

- *(deps)* bump agent-client-protocol to 1.0 and reqwest to 0.13
- version manch-protocol independently at 0.0.1; mark apps no-publish
- serde JSON round-trip property tests for protocol contract
- Scaffold workspace and manch-protocol; rewrite README
