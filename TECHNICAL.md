# Technical Overview — Kernel Engine

This document describes the architectural principles, language design, and technical direction of Kernel Engine. It is intended for contributors, reviewers, and engineers evaluating the project's approach. No code samples are included at this stage — the focus is on conceptual design and rationale.

---

## Language Design

Kernel Engine is a neutral-core DSL. The language itself contains no built-in knowledge of Linux, operating systems, or any specific technology. Its core provides general-purpose constructs — loops, conditionals, variable declarations, class definitions, and inheritance — and nothing else. Every OS-specific concept (disk partitions, filesystem layouts, bootloader configuration, kernel command-line parameters, init system selection and configuration, service definitions, user accounts, firewall rules, SELinux labels) must be declared explicitly by the user or inherited from a class that a user or community author has written.

This design is deliberate. By refusing to embed OS assumptions into the language, Kernel Engine ensures that every property of a built system is visible in source code. There are no hidden defaults, no implicit behaviors, and no configuration that exists outside the declared source tree. The system a Kernel Engine file describes is exactly and only what that file contains. Over time, standard class libraries will provide sensible defaults and reusable building blocks for common configurations, but these classes are themselves written in Kernel Engine and are fully inspectable — they are convenience, not magic.

Classes follow an object-oriented model with single inheritance. A base class might define a minimal server configuration — partitioning scheme, bootloader, kernel parameters, a locked-down user model — and derived classes extend or override specific properties to produce specialized roles such as a web server, database host, or monitoring node. This structure eliminates repetition across fleet definitions while preserving the ability to inspect and override any inherited property at any layer of the hierarchy.

---

## Outputs

The compiler's output evolves across development phases. In Phase 1, output is limited to validation results, diagnostic logs, and the parsed internal representation of the source tree. Beginning in Phase 2, the compiler targets bootable ISO image generation from Fedora and AlmaLinux minimal bases. Intermediate output formats — configuration bundles, structured declarations suitable for external tooling — are planned as the project matures.

Future output capabilities include auditing hooks (structured reports comparing declared state against live system state) and basic orchestration primitives (state-aware redeployment where only the delta between declared and current state is applied). These capabilities are planned for Phase 4 and beyond.

---

## Low-Level Granularity

Kernel Engine provides raw, unmediated control over every layer of system construction. Users script disk partitioning operations (fdisk, parted, or equivalent tooling invoked through declared commands), specify kernel command-line parameters directly, select and configure any init system (systemd, OpenRC, s6, runit, or others) without language-level bias, define service supervision trees, manage bootloader installation and configuration, and control filesystem permissions and ownership at arbitrary granularity.

The language imposes no preference on any of these choices. A Kernel Engine source file that specifies s6 as the init system is as natural and well-supported as one that specifies systemd — the language does not distinguish between them. Classes provide the mechanism for abstracting common patterns (a "hardened systemd server" class, an "s6 container host" class) so that users can work at the level of abstraction appropriate to their task without losing the ability to drop to raw declarations when needed.

---

## File Handling

Kernel Engine includes built-in primitives for reading and writing structured and unstructured data files. Supported formats include YAML, JSON, plaintext, and encrypted files. This capability enables integration with existing infrastructure data — inventories, secrets vaults, environment-specific parameters — without requiring external preprocessing or templating tools. File I/O is available both at compile time (for parameterizing builds from external data) and as a declared system operation (for generating configuration files on the target system during build).

---

## Security Model

Security in Kernel Engine operates at three layers: source integrity, compile-time validation, and runtime auditing.

Source integrity is enforced by design. A Kernel Engine source tree is the single, authoritative definition of a system's configuration. Because source files are plain text, they are naturally suited to Git version control. Every change to a system's definition is tracked, attributable, and reversible. The source tree is immutable in the sense that a given commit always describes the same system — there is no external state, no database, and no runtime-only configuration that could diverge from what is recorded in version control.

Compile-time validation catches misconfigurations before they produce images. The compiler validates structural correctness (well-formed declarations, type consistency, class resolution) and, as the project matures, will validate semantic correctness (conflicting mount points, unreachable network configurations, invalid SELinux label combinations). The goal is to surface errors at build time rather than at boot time.

Runtime auditing (Phase 4+) compares the live state of a deployed system against the source code that produced it. Any property that has changed — a modified file, an added user, a disabled service, an altered firewall rule — is surfaced as drift. This provides continuous compliance visibility without requiring a separate configuration management tool running on the target system.

SELinux Multi-Level Security (MLS) is a likely early candidate for a standard class library, given the project's security-first orientation. Filesystem labeling and policy generation are well-suited to declarative specification. TPM integration (measured boot, sealed secrets) is expected to rely on external tooling invoked through declared commands rather than language-level primitives, as TPM operations are hardware-specific and better served by purpose-built utilities.

---

## Backend

The compiler is implemented in Rust. This choice provides memory safety without garbage collection overhead, strong type system guarantees that reduce compiler bugs, and high performance for parsing and image generation workloads. The parser implementation is not yet finalized — pest (PEG-based) and nom (parser combinator) are both under consideration, and the decision will be made during Phase 1 based on ergonomics and error-reporting quality. Serialization and deserialization of structured data (YAML, JSON) will use the serde ecosystem, which is the de facto standard in Rust for this purpose.

---

## Extensibility

Kernel Engine is designed to grow through community contribution rather than monolithic feature expansion. The class system is the primary extensibility mechanism: new distribution support, new service configurations, new security postures, and new deployment patterns are all expressible as class libraries without changes to the language or compiler.

Planned future extensibility includes output adapters for external tools (Ansible playbook generation, cloud-init compatibility), additional distribution targets beyond Fedora and AlmaLinux (Debian and Arch are likely early candidates), and a community class repository for sharing and discovering reusable system definitions.
