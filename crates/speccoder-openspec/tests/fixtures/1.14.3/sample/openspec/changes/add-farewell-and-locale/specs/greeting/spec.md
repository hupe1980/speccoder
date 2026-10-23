## ADDED Requirements

### Requirement: Farewell
The system SHALL print `Goodbye, <name>!` when invoked with `--bye`.

#### Scenario: Farewell with name
- **WHEN** the user runs `greet --bye Ada`
- **THEN** the output is `Goodbye, Ada!`

## MODIFIED Requirements

### Requirement: Default greeting
The system MUST greet the value of `GREET_DEFAULT` when no name argument is given, and "World" when it is unset.

#### Scenario: No name given
- **WHEN** the user runs `greet` with no arguments and `GREET_DEFAULT` is unset
- **THEN** the output is `Hello, World!`

#### Scenario: Configured default
- **GIVEN** `GREET_DEFAULT=Grace`
- **WHEN** the user runs `greet` with no arguments
- **THEN** the output is `Hello, Grace!`

## RENAMED Requirements

- FROM: `### Requirement: Greet a named person`
- TO: `### Requirement: Greet by name`
