# Design

## Context

Greenfield CLI. See proposal.md - Why.

## Goals / Non-Goals

**Goals:**
- Single binary, no config.

**Non-Goals:**
- Localization.

## Decisions

- Plain argv parsing; no CLI framework (one optional positional argument).

## Risks / Trade-offs

- [Unicode names] → print as given, no normalization.
