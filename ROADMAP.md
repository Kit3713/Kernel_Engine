# Roadmap — Kernel Engine

This roadmap outlines phased development from pre-alpha through full release. Versions follow semantic versioning: alpha releases span 0.1.x through 0.4.x, beta begins at 0.5.x, and the first stable release is 1.0. Early phases prioritize concept validation and architectural correctness; later phases shift focus toward usability, reliability, and ecosystem growth.

---

## Phase 1 — Core Parser and Basic Compiler (0.1.x)

- Define and formalize the Kernel Engine grammar specification.
- Implement the core parser capable of tokenizing and validating Kernel Engine source files against the grammar.
- Produce structured compiler output: syntax validation results, diagnostic logs, and parsed representation of the source tree.
- No OS image generation at this stage — the objective is a correct, tested parser that accepts valid Kernel Engine code and rejects invalid input with clear error reporting.

## Phase 2 — Proof of Concept (0.2.x)

- Generate a primitive bootable ISO from a Kernel Engine source file targeting a Fedora or AlmaLinux minimal base.
- Validate the end-to-end pipeline: source file → parsed representation → system image containing declared partitions, bootloader, kernel parameters, init configuration, and basic services.
- Output may be minimal and rough — the goal is to prove that the language-to-image compilation path is functional and architecturally sound.

## Phase 3 — Object-Oriented Foundation (0.3.x)

- Implement the class system: class definitions, instantiation, inheritance, and method/property resolution.
- Introduce variables and parameterization to support fleet-wide variation from a shared source tree.
- Enable rapid iteration on system definitions without raw repetition — users define base classes and extend or override for specific roles, hosts, or environments.

## Phase 4 — Auditability (0.4.x)

- Implement runtime state comparison: read the live state of a deployed system and compare it against the declared source code that produced it.
- Surface drift as structured output — identify which properties have changed, what the declared values are, and what the current live values are.
- Lay the groundwork for continuous compliance workflows where deployed systems are periodically validated against their source definitions.

## Beta — Maturity and Stability (0.5.x+)

- Develop and publish robust standard class libraries for common system configurations: hardened server bases, desktop environments, network services, and security postures including SELinux policy profiles.
- Harden the compiler and runtime against edge cases, malformed input, and adversarial configurations.
- Reduce the amount of manual declaration required for typical builds by providing well-tested, composable classes — without sacrificing the ability to override any default at any layer.

## Full Release (1.0)

- Deliver a mature, auditable, declarative Linux build system capable of producing reproducible systems across desktop and server workloads.
- Comprehensive documentation, stable grammar specification, and a tested standard class library.
- The compiler and runtime are considered production-grade for the supported target distributions (Fedora, AlmaLinux).

## Post-1.0

- Community-driven expansion: additional distribution targets (Debian, Arch, and others), desktop and server cluster support, and update/iteration mechanisms for managing deployed fleets over time.
- Explore integration points with external orchestration and trust verification tooling as the ecosystem matures.

---

Early phases prioritize concept validation; later ones focus on usability and ecosystem.
