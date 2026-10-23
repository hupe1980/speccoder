# Design

## Context

Extends the existing `greet` binary.

## Goals / Non-Goals

**Goals:**
- Keep argv parsing trivial.

**Non-Goals:**
- Config files.

## Decisions

- Read `GREET_DEFAULT` once at startup.

## Risks / Trade-offs

- [Empty `GREET_DEFAULT`] → treat as unset.
