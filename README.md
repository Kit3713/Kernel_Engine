# Kernel Engine

**A domain-specific language and compiler for declaratively building secure, reproducible Linux systems from minimal ISOs.**

---

## Overview

Kernel Engine is a neutral-core domain-specific language (DSL) paired with a Rust-based compiler, designed to give engineers complete declarative control over every layer of a Linux system — from disk partitioning and bootloader configuration through kernel parameters, init systems, services, users, and security labels. Unlike configuration management tools that assume a running system and drift over time, or high-level declarative platforms that impose opinionated abstractions, Kernel Engine starts from nothing: a minimal ISO and a source file that *is* the system.

The project exists to fill a gap in the Linux ecosystem. Nix and NixOS offer reproducibility but impose a steep learning curve and a functional paradigm that obscures low-level control. Ansible and its peers manage state atop existing systems but cannot prevent drift without continuous enforcement. Linux From Scratch provides total control but at the cost of entirely manual, unversioned labor. Kernel Engine occupies the space between these approaches: a language with no built-in Linux knowledge, where every OS primitive — partitions, mount points, kernel command-line flags, init configuration, firewall rules, SELinux labels — is defined explicitly in code or inherited from reusable classes.

The result is a system whose identity is its source code. Every build is reproducible, every configuration decision is auditable, and every deployed system can be compared against its declared state for drift detection. Security is a first-class concern: compile-time validation catches misconfigurations before they reach hardware, runtime auditing surfaces unauthorized changes, and structured class libraries make complex security postures like SELinux MLS enforceable without manual policy authoring. Kernel Engine is currently in pre-alpha (v0.0.1) — this repository contains the project vision and architectural skeleton only.

---

## Features

- **Low-level granularity with no assumptions.** The language core contains no Linux-specific knowledge. Users declare partitions, bootloaders, kernel parameters, init systems, services, and security labels explicitly — nothing is implied or hidden.
- **Object-oriented class system with inheritance.** Reusable classes encapsulate common configurations (e.g., a hardened server base, a desktop environment profile) and can be extended or overridden, eliminating repetition without sacrificing transparency.
- **Built-in file I/O.** Native support for reading and writing YAML, JSON, plaintext, and encrypted files enables integration with existing configuration data and secrets management workflows.
- **Auditing and drift detection.** Runtime comparison of live system state against declared source code surfaces unauthorized changes and configuration drift, providing continuous compliance visibility.
- **Variables for fleet-wide iteration.** Parameterized builds allow a single source tree to target multiple hosts, roles, or environments with controlled variation — no templating engine required.
- **Distro-agnostic design.** Initial development targets Fedora and AlmaLinux minimal ISOs; the architecture supports future expansion to Debian, Arch, and other distributions without language changes.
- **Rust compiler backend.** The compiler is implemented in Rust for memory safety, performance, and correctness — critical properties for a tool that generates bootable operating systems.

---

## Status & Version

**Pre-alpha v0.0.1** — concept phase. This repository contains the project vision, architectural documentation, and initial scaffolding. Alpha development begins at **0.1.0** with the core parser and syntax validation.

---

## Target Audience

Kernel Engine is designed for security administrators, Linux specialists, and DevOps engineers who require low-level declarative power over system builds — professionals who find Nix too opinionated, Ansible too drift-prone, and manual methods too fragile for production-grade, auditable infrastructure.

---

## Roadmap

Development is organized into phased milestones from pre-alpha through stable release. See [ROADMAP.md](./ROADMAP.md) for the full plan.

---

## Contributing

Contributions are welcome at every stage. Fork the repository and submit pull requests for grammar definitions, example configurations, or architectural design proposals. Issues are encouraged for vision feedback, feature requests, and discussion of design decisions.

---

## License

This project is licensed under the MIT License. See [LICENSE](./LICENSE) for details.
