# Ironclad

**A declarative language and compiler for building, auditing, and maintaining secure, reproducible Linux systems across their full lifecycle.**

---

## Overview

Ironclad is a domain-specific language (DSL) paired with a Rust-based compiler and a runtime agent, designed to serve as the unified declaration layer for every aspect of a Linux system — from disk partitioning and bootloader configuration through kernel parameters, init systems, services, users, firewall rules, and SELinux policy — across the full system lifecycle from blank disk to ongoing runtime compliance.

The Linux ecosystem already contains capable tools for individual parts of this problem. bootc manages image lifecycle. Kickstart handles installer-time disk configuration. osbuild constructs images. SELinux provides mandatory access control. nftables enforces network policy. Ansible mutates running systems. None of them share a language, a validation model, or a source of truth. None of them know about each other. The result is that a real production system's definition is scattered across Containerfiles, Kickstart scripts, osbuild blueprints, hand-authored SELinux policy, and Ansible playbooks — with no single place to read what the system is supposed to be, no compile-time guarantee that the pieces are consistent, and no runtime mechanism to detect when reality has diverged from intent.

Ironclad occupies the layer above these tools. A single Ironclad source tree declares every property of a system. The compiler validates that declaration for structural and semantic correctness, then emits the appropriate artifacts for each backend: a bootc Containerfile for image lifecycle, a Kickstart configuration for disk layout and installation, SELinux MLS policy generated from the declared system topology, nftables rules, s6 or systemd service trees, and a signed intermediate manifest baked into the image that the runtime agent uses for continuous drift detection. Runtime maintenance is handled by compiling the delta between two Ironclad declarations into an Ansible playbook applied to the running system.

The result is a system whose identity is its source code — auditable, reproducible, and continuously verified against its declared state.

---

## The Problem With Existing Tools

**bootc** manages image updates and rollbacks but does not build images. System definitions live in Containerfiles, which are inherently imperative — `RUN` commands with no semantic validation, no type checking, and no guarantee that a silent failure doesn't produce a subtly broken image. Disk layout, encryption, and TPM binding are entirely outside bootc's scope, handled separately at install time with no connection to the image definition.

**osbuild / Image Builder** constructs images from a blueprint format that covers packages, users, enabled services, and kernel arguments. Complex declarations — arbitrary SELinux policy, fine-grained file permissions, custom init supervision trees, detailed firewall rules — fall outside the blueprint schema and require shell scripts, abandoning the declarative model entirely. There is no inheritance or reuse mechanism; every blueprint is flat.

**Butane / Ignition** provides YAML-based first-boot configuration for CoreOS and RHCOS. It is not a language — no variables, no loops, no conditionals, no inheritance. Parameterizing for multiple environments requires external templating. It is scoped to first-boot and assumes the base image already exists elsewhere.

**Kickstart** is the most honest of the group: a one-shot installer format with declarative syntax for the easy parts and raw bash for everything else. No reuse, no validation, no type system. Once installation completes, the system is orphaned from its Kickstart definition with no ongoing relationship and no drift detection.

**SELinux policy authoring** is notoriously manual, error-prone, and disconnected from every other system declaration tool. Writing correct MLS policy requires deep expertise and produces artifacts that have no formal relationship to the system topology that policy is supposed to govern.

None of these tools can together express disk layout, encryption, bootloader, kernel parameters, init system, services, SELinux policy, firewall rules, and users in a single unified source of truth with a real language, semantic validation, and lifecycle-spanning drift detection. That is the gap Ironclad fills.

---

## How It Works

Ironclad source files declare a complete system using a structured DSL with an object-oriented class system supporting single inheritance. Classes encapsulate reusable configurations — a hardened server base, an SELinux MLS workstation profile, an s6-supervised container host — which derived declarations extend or override for specific roles and environments.

The compiler processes source files through several stages: parsing and AST construction, class resolution and inheritance flattening, semantic validation (conflicting mount points, invalid SELinux label combinations, unreachable network configurations, security floor enforcement), and backend emission. Backend emission produces the full set of artifacts required to build, install, and manage the declared system:

- A **bootc Containerfile** for OCI image construction and lifecycle management
- A **Kickstart configuration** covering disk partitioning, LUKS2 encryption, LVM, TPM2/Clevis binding, and bootloader installation
- **SELinux MLS policy** generated from the declared system topology, with configurable strictness levels and full manual override capability
- **nftables rules** derived from declared network policy
- **Init system configuration** (s6 service trees or systemd units) for declared services
- A **signed intermediate manifest** — a fully resolved, backend-agnostic serialization of the compiled system state — baked into the image as the ground truth for runtime auditing

The runtime agent, embedded in every Ironclad-built image at compile time, periodically compares live system state against the intermediate manifest. Any divergence — a modified file, an added user, a changed firewall rule, an altered SELinux label — is reported as structured drift output. No separate configuration management tool is required on the target system.

Runtime maintenance (post-deployment changes) is handled by diffing two Ironclad declarations at the AST level and emitting an Ansible playbook representing the delta. After the playbook is applied, the agent verifies that live state matches the new declaration.

---

## SELinux MLS Policy Generation

SELinux Multi-Level Security policy authoring is one of the most technically demanding and error-prone tasks in hardened Linux system administration. Ironclad treats SELinux as a first-class concern rather than an afterthought.

When SELinux MLS mode is enabled in a system declaration, the compiler uses the full declared system topology — processes, services, files, network interfaces, users, and their relationships — to generate a correct baseline MLS policy. Strictness is configurable: a single compiler flag shifts the generated policy from a permissive baseline suitable for development to a maximally restrictive posture appropriate for classified or high-security environments.

The generated policy is fully transparent and inspectable. After compilation, engineers can review the generated policy, modify it manually, or override specific rules entirely in source. The compiler will incorporate manual overrides on subsequent builds. Engineers who prefer to author policy entirely by hand can do so — the SELinux generator is an accelerator, not a requirement.

This model makes correct MLS policy achievable for organizations that need it but lack the specialist policy authoring expertise it traditionally requires.

---

## Features

- **Unified lifecycle declaration.** A single Ironclad source tree covers build time, install time, and runtime — no scattered artifacts across disconnected tools.
- **Backend integration, not replacement.** Ironclad compiles to bootc, Kickstart, osbuild, nftables, and Ansible. It leverages the RHEL ecosystem's existing tooling rather than reinventing it.
- **Object-oriented class system with inheritance.** Reusable classes encapsulate common configurations and can be extended or overridden at any layer. Fleet-wide patterns are expressed once; role-specific variations override only what differs.
- **SELinux MLS policy generation.** Compiler-generated MLS policy from declared system topology, with configurable strictness and full manual override capability.
- **Compile-time semantic validation.** Misconfigurations are caught before they produce images — conflicting mount points, invalid label combinations, security floor violations.
- **Runtime drift detection.** An embedded agent continuously compares live system state against the compiled manifest that produced it, surfacing unauthorized changes as structured output.
- **Ansible-backed runtime maintenance.** Declaration deltas compile to Ansible playbooks for safe, idempotent application to running systems.
- **Signed intermediate manifest.** The compiled system state is serialized, signed, and baked into every image as the authoritative ground truth for auditing.
- **RHEL ecosystem first.** Initial targets are Fedora and AlmaLinux minimal bases. Debian and Arch support are planned.

---

## Status & Version

**Pre-alpha v0.0.1** — concept and architecture phase. This repository contains the project vision, architectural documentation, and initial scaffolding. No code is present at this stage. Alpha development begins at 0.1.0 with the core parser and grammar.

---

## Target Audience

Ironclad is designed for security administrators, Linux platform engineers, and DevOps engineers in environments where system auditability, reproducibility, and compliance are non-negotiable — particularly defense, government, and regulated industry contexts where SELinux MLS enforcement, drift detection, and a complete chain of custody from declaration to running system are required.

---

## Roadmap

Development is organized into phased milestones from pre-alpha through stable release. See [ROADMAP.md](ROADMAP.md) for the full plan.

---

## Contributing

Contributions are welcome at every stage. Fork the repository and submit pull requests for grammar definitions, class library designs, backend emitter proposals, or architectural feedback. Issues are encouraged for design discussion and feature proposals.

---

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.
