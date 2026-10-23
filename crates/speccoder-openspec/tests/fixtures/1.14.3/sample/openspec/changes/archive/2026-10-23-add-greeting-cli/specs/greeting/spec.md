# Spec Delta

## Purpose

Lets users print friendly greetings for a person from the command line.

## ADDED Requirements

### Requirement: Greet a named person
The system SHALL print `Hello, <name>!` when invoked with a name argument.

#### Scenario: Name given
- **WHEN** the user runs `greet Ada`
- **THEN** the output is `Hello, Ada!`
- **AND** the exit code is 0

### Requirement: Default greeting
The system MUST greet "World" when no name argument is given.

#### Scenario: No name given
- **WHEN** the user runs `greet` with no arguments
- **THEN** the output is `Hello, World!`
