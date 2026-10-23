# Proposal

## Why

Users also want to say goodbye, and the default greeting should honour a configured name.

## What Changes

- Add a `--bye` flag that prints a farewell.
- The default name comes from `GREET_DEFAULT` when set.
- Rename "Greet a named person" to "Greet by name".

## Capabilities

### New Capabilities

### Modified Capabilities
- `greeting`: adds farewell, changes default-name behaviour.

## Impact

`greet` binary only.
