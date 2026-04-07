# Roadmap — Ironclad

Development is organized into phased milestones from pre-alpha through stable release. Versions follow semantic versioning: alpha releases span 0.1.x through 0.5.x, beta begins at 0.6.x, and the first stable release is 1.0. Early phases prioritize architectural correctness and a working end-to-end pipeline; later phases expand the standard class library, harden the compiler, and deliver the full runtime model.

---

## Phase 1 — Core Parser and Compiler Skeleton (0.1.x)

- Define and formalize the Ironclad grammar specification using pest (PEG).
- Implement the core parser: tokenization, AST construction, and syntax validation against the grammar.
- Implement the class resolution pass: inheritance flattening, property resolution, and origin tracking.
- Produce structured compiler output: diagnostic logs, the resolved AST, and a human-readable representation of the flattened system declaration.
- No backend emission at this stage. The objective is a correct, well-tested parser and resolver that accepts valid Ironclad source and rejects invalid input with precise, actionable error reporting.

## Phase 2 — Backend Emission and Proof of Concept (0.2.x)

- Implement the intermediate manifest: CBOR serialization of the resolved AST, Ed25519 signing, and manifest verification.
- Implement the bootc Containerfile emitter: generate a valid Containerfile from a declared system definition targeting a Fedora or AlmaLinux minimal base.
- Implement the Kickstart emitter: generate a Kickstart configuration covering disk partitioning, LUKS2 encryption, LVM, TPM2/Clevis binding, bootloader installation, and kernel command-line parameters.
- Validate the end-to-end build pipeline: Ironclad source → compiler → Containerfile + Kickstart → bootable installed system.
- Ship the initial standard class library: `HardenedRHELBase`, `S6ContainerHost`, `SystemdServer`, and at minimum one SELinux MLS profile. Standard library classes must be written in Ironclad and fully inspectable.

## Phase 3 — Semantic Validation (0.3.x)

- Implement the semantic validation pass with domain-aware rules: conflicting mount points, invalid SELinux label combinations, services declared without a corresponding init system, firewall rules referencing undeclared interfaces, security floor enforcement (SELinux enforcing mode required, LUKS2 required, minimum immutability).
- Implement the nftables emitter: generate a complete nftables ruleset from declared network policy.
- Implement init system emitters: s6 service tree generation and systemd unit file generation, selected by the declared init system.
- Inheritance depth warnings and other compiler ergonomics.

## Phase 4 — SELinux MLS Policy Generation (0.4.x)

- Implement the SELinux MLS policy generator: analyze the resolved AST system topology and generate `.te`, `.fc`, and `.if` policy files using the Reference Policy as a foundation.
- Implement strictness levels: `baseline`, `standard`, `high`, and `maximum`, producing policies of graduated restrictiveness from the same declaration.
- Implement manual override handling: preserve engineer-authored policy modifications across recompilation, flagging conflicts when declaration changes invalidate existing overrides.
- Validate generated policy against declared topology for internal consistency before emission.
- Ship additional standard library SELinux profiles covering common server roles.

## Phase 5 — Runtime Agent and Drift Detection (0.5.x)

- Implement the Ironclad runtime agent in Rust: manifest reading and verification, live state comparison, structured drift reporting (JSON to configurable sinks).
- Define the checked property set: file content hashes and permissions, user accounts and group memberships, running service states, loaded firewall rules, active SELinux labels on monitored paths.
- Embed the agent in images at build time via the Containerfile emitter.
- Implement post-maintenance verification: triggered agent comparison after Ansible playbook application, convergence reporting.
- No remediation in the agent — detection and reporting only.

## Phase 6 — Runtime Maintenance (0.6.x / Beta)

- Implement the AST delta engine: accept two Ironclad source trees (or two Git refs) and produce a structured diff at the AST level.
- Implement the Ansible playbook emitter: translate AST deltas into idempotent Ansible playbooks using standard modules (ansible.builtin.*, ansible.posix.*).
- Validate the maintenance pipeline: declaration change → compiler delta → generated playbook → agent verification.
- Implement the osbuild blueprint emitter as an alternative image backend for air-gapped and bare-metal environments where bootc is not available.
- Harden the compiler and agent against edge cases, malformed input, and adversarial configurations.

## Full Release (1.0)

- Complete, production-grade compiler covering all validation passes and all first-class backends (bootc, Kickstart, nftables, SELinux MLS, s6/systemd, Ansible delta).
- Runtime agent stable and suitable for production deployment.
- Comprehensive standard class library covering hardened server bases, SELinux MLS profiles for common workloads, container host configurations, and network service roles.
- Complete documentation: language reference, grammar specification, class authoring guide, backend integration guide, runtime agent configuration.
- The compiler and runtime are considered production-grade for Fedora and AlmaLinux targets.

## Post-1.0

- Additional distribution targets: Debian and Arch are the likely first candidates, requiring new backend emitters for apt-based package management and distribution-specific conventions.
- Community class repository: a curated index of contributed Ironclad class libraries for sharing and discovering reusable system definitions.
- Fleet topology primitives: declarations spanning multiple hosts with cluster-aware validation (network reachability, service dependency graphs across nodes).
- Expanded output adapters: cloud-init compatibility for cloud deployments, Terraform provider for infrastructure-as-code integration.
- RHEL proper support when resources permit.

---

Early phases prioritize a correct, working end-to-end pipeline over feature breadth. Later phases build on that foundation with the full semantic validation, SELinux generation, and runtime model that define Ironclad's identity.
