# Technical Overview — Ironclad

This document describes the architectural principles, language design, compiler pipeline, backend integration strategy, and runtime model of Ironclad. It is intended for contributors, reviewers, and engineers evaluating the project's approach.

---

## Architecture Overview

Ironclad operates across four distinct modes that together span the full system lifecycle:

**Build time** — The compiler parses Ironclad source, resolves the class hierarchy, performs semantic validation, and emits backend artifacts. This is a pure static pipeline: source goes in, artifacts come out. No running system is involved.

**Install time** — The emitted Kickstart configuration drives Anaconda to partition disks, configure LUKS2 encryption, bind TPM2/Clevis, install the bootloader, and bootstrap the system from the bootc-managed OCI image. The signed intermediate manifest is written to the installed system during this phase.

**Runtime auditing** — The Ironclad runtime agent, embedded in the image at build time, periodically reads the signed intermediate manifest and compares it against live system state. Drift is reported as structured output.

**Runtime maintenance** — When the system declaration changes, the compiler diffs the old and new ASTs and emits an Ansible playbook representing the delta. After the playbook is applied, the agent verifies that live state matches the new manifest.

---

## Language Design

Ironclad is a structured, statically-typed DSL with explicit domain knowledge of Linux system primitives. Unlike a neutral general-purpose language, Ironclad's type system and grammar understand partitions, mount points, kernel parameters, init systems, service definitions, user accounts, firewall rules, and SELinux labels as first-class constructs. This domain awareness enables the compiler to perform meaningful semantic validation — catching conflicting mount points, invalid SELinux label combinations, and security floor violations at compile time rather than at boot time or in production.

The language provides general-purpose constructs — variables, loops, conditionals, and class definitions — but these operate over domain-typed values. A variable is not just a string; it is a typed declaration that the compiler can validate in context.

### Class System

Ironclad uses a single-inheritance object-oriented class system. A base class declares a complete or partial system configuration. Derived classes extend or override specific properties to produce specialized roles. The full class hierarchy is flattened by the compiler during the resolution pass; the resulting AST contains no unresolved inheritance — every property has an explicit, traceable value.

This model is intentionally chosen over a functional approach (as in Nix) for two reasons: it maps naturally to the organizational thinking of infrastructure teams who reason about roles and role hierarchies, and it makes the inheritance chain inspectable at any layer without requiring fluency in a functional paradigm. The tradeoff — that deep inheritance hierarchies can become difficult to reason about — is managed by convention: the standard class library keeps hierarchies shallow, and the compiler emits warnings when inheritance depth exceeds a configurable threshold.

### Standard Class Library

The class system is only as useful as the classes available to users. The standard library ships with Ironclad from Phase 2 onward and provides vetted, composable base classes for common configurations: hardened RHEL server bases, SELinux MLS workstation profiles, s6-supervised container hosts, systemd server roles, and others. Standard library classes are written in Ironclad, fully inspectable, and overridable at any property.

---

## Compiler Pipeline

### Stage 1 — Parsing

The parser reads Ironclad source files and produces an abstract syntax tree (AST). The parser is implemented in Rust using a PEG grammar (pest). The grammar is the canonical specification of valid Ironclad syntax. Invalid input is rejected with structured diagnostic output indicating the location and nature of the error.

### Stage 2 — Class Resolution

The compiler traverses the class hierarchy, resolving inheritance relationships and flattening derived classes into fully specified AST nodes. After this pass, every declared system property has an explicit value with a traceable origin in the source tree.

### Stage 3 — Semantic Validation

The compiler applies domain-aware validation rules against the resolved AST. Current planned validations include: conflicting mount point declarations, invalid SELinux label combinations, services declared without a corresponding init system, firewall rules referencing undeclared interfaces, security floor enforcement (SELinux enforcing mode, LUKS2 encryption presence), and TPM2 binding declared without a compatible bootloader configuration. Semantic validation is extensible — new rules are added as the compiler matures without changes to the grammar.

### Stage 4 — Intermediate Manifest Generation

The compiler serializes the fully resolved AST into a signed intermediate manifest. This manifest is the canonical, backend-agnostic representation of the declared system state. It is written into every built image and serves as the ground truth for the runtime agent. The manifest format is CBOR with an attached Ed25519 signature over the manifest content. The signing key is generated per build and the public key is stored in the manifest header for agent verification.

### Stage 5 — Backend Emission

The compiler emits backend-specific artifacts from the resolved AST and intermediate manifest:

**bootc Containerfile** — Declares the OCI image layers, package installation, file drops, and service configuration. The Containerfile is the input to the bootc image build pipeline and governs image lifecycle (updates, rollbacks, pinning).

**Kickstart configuration** — Covers everything bootc cannot: disk partitioning scheme, LUKS2 configuration, LVM volume groups and logical volumes, TPM2/Clevis binding commands, bootloader installation and kernel command-line parameters. The Kickstart `%post` section is generated, not hand-written, and is kept minimal — complex configuration lives in the image, not the installer.

**SELinux MLS policy** — See the SELinux section below.

**nftables ruleset** — Generated from declared network policy. The ruleset is a complete, self-contained nftables configuration applied during image build and validated for consistency against declared network interfaces and services.

**Init system configuration** — Either an s6 service tree (compiled from declared service supervision relationships) or systemd unit files, depending on the declared init system. The compiler does not prefer one over the other.

**osbuild blueprint** (alternative backend) — For environments where bootc is not appropriate (air-gapped, bare metal without OCI infrastructure), the compiler can target osbuild's blueprint format instead of a Containerfile.

---

## SELinux MLS Policy Generation

SELinux Multi-Level Security (MLS) policy authoring is treated as a first-class compiler output rather than a separate manual task.

When `selinux_mls: enabled` is declared in a system definition, the compiler analyzes the fully resolved AST to extract the system's process topology, service relationships, filesystem layout, network interfaces, and user account structure. From this topology, the compiler generates a complete baseline MLS policy using the Reference Policy as a foundation, augmented with type enforcement rules derived from the declared system structure.

Strictness is controlled by a single declaration:

```
selinux_mls {
  enabled: true
  strictness: high   # options: baseline | standard | high | maximum
}
```

`baseline` generates a permissive-leaning policy suitable for development and initial deployment. `maximum` generates the most restrictive policy the declared topology permits — all unlabeled access denied, all inter-process communication explicitly allowed, MLS clearance ranges minimized to declared requirements. Intermediate levels provide graduated postures between these extremes.

The generated policy is emitted as a human-readable set of `.te`, `.fc`, and `.if` files alongside the other compiler outputs. Engineers can inspect, modify, or extend the generated policy before building. Manual modifications are preserved across subsequent compilations unless the underlying declaration changes in a way that invalidates them, in which case the compiler flags the conflict rather than silently overwriting. Engineers who prefer to author policy entirely by hand can disable generation and provide their own policy files; the compiler incorporates them into the build without modification.

---

## Runtime Agent

The Ironclad runtime agent is a lightweight daemon compiled into every Ironclad-built image. It performs two functions:

**Drift detection** — On a configurable schedule, the agent reads the signed intermediate manifest and compares declared property values against live system state. Checked properties include file content hashes and permissions, active user accounts and group memberships, running services and their states, loaded firewall rules, and active SELinux labels on monitored paths. Any divergence is emitted as a structured JSON report to a configurable sink (local file, syslog, remote endpoint).

**Post-maintenance verification** — After an Ansible maintenance playbook is applied, the agent performs an immediate full comparison and reports whether live state now matches the updated manifest. This closes the feedback loop on runtime changes.

The agent reads but never writes. It does not remediate drift — it detects and reports it. Remediation is a deliberate human or automation decision made outside the agent.

---

## Runtime Maintenance Model

When a system declaration changes, the Ironclad compiler in delta mode accepts two source trees (or two Git refs) and produces a diff at the AST level. This diff is translated into an Ansible playbook that idempotently applies only the changed properties to the running system. The playbook is generated, not hand-written, and uses standard Ansible modules (ansible.builtin.file, ansible.builtin.user, ansible.posix.firewalld, etc.) to ensure correct, tested behavior.

After playbook application, the runtime agent verifies convergence. If the agent reports residual drift, the maintenance run is flagged as incomplete and the specific unresolved properties are identified for manual review.

This model avoids reinventing the "safely mutate a running system" problem, which Ansible has solved with extensive testing across the RHEL ecosystem.

---

## Backend Design Philosophy

Ironclad does not compete with its backends. bootc, Kickstart, osbuild, nftables, s6, systemd, and Ansible are mature tools that solve specific problems well. Ironclad's value is in the layer above them: a unified declaration language with real language features (variables, loops, conditionals, inheritance), domain-aware semantic validation, and a lifecycle-spanning source of truth that none of these tools provide individually or in combination.

Adding support for a new backend does not require language changes — it requires a new emitter module that translates the intermediate manifest into the target format. This architecture allows Ironclad to expand to new distributions, new init systems, and new deployment targets without breaking existing declarations.

---

## Rust Implementation

The compiler is implemented in Rust. This provides memory safety without garbage collection, a strong type system that reduces compiler bugs, and the performance required for parsing large source trees and generating image artifacts. The parser uses the pest crate (PEG-based) for its strong error reporting and readable grammar definitions. Structured data serialization uses serde with CBOR output for the intermediate manifest and JSON for diagnostic and drift reports. Cryptographic signing uses the ed25519-dalek crate.
