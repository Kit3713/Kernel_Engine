# Phase 2 — Backend Emission and Proof of Concept (0.2.x)

Phase 2 delivers the first end-to-end build pipeline: Ironclad source goes in, a bootable installed system comes out. This phase adds the intermediate manifest, the bootc Containerfile emitter, the Kickstart emitter, and the initial standard class library. The milestone is complete when a single Ironclad source file compiles to a Containerfile and a Kickstart configuration that together produce a bootable, installed Fedora or AlmaLinux system with LUKS2 encryption and a signed manifest on disk.

**Prerequisite:** Phase 1 complete (0.1.x). The parser, class resolver, CLI, and diagnostic infrastructure are working and tested.

---

## 2.1 — Intermediate Manifest: Data Model and Serialization

**Version: 0.2.0-dev**

Define the manifest format — the backend-agnostic serialization of a resolved system that every emitter reads from and the runtime agent will eventually verify against.

**Deliverables:**

- Define the `Manifest` struct in `ironclad-ast` (or a new `ironclad-manifest` crate): a serializable representation of the `ResolvedSystem` stripped of source spans and compiler internals, containing only the declared system state.
- Choose and document the canonical field ordering and naming conventions for the manifest schema.
- Implement CBOR serialization and deserialization using `serde` and `ciborium`.
- Write a function `resolved_to_manifest(resolved: &ResolvedSystem) -> Manifest`.
- Write round-trip tests: resolve a system, serialize to CBOR, deserialize, assert equality.
- Write a `manifest_version` field into the schema (integer, starting at 1) for forward compatibility.

**Done when:** A `ResolvedSystem` serializes to CBOR and deserializes back identically.

---

## 2.2 — Intermediate Manifest: Ed25519 Signing and Verification

**Version: 0.2.0-dev**

Sign the manifest so the runtime agent can later verify it hasn't been tampered with.

**Deliverables:**

- Add `ed25519-dalek` and `rand` dependencies.
- Implement `sign_manifest(manifest_cbor: &[u8]) -> SignedManifest` which generates an Ed25519 keypair, signs the CBOR bytes, and produces a `SignedManifest` containing: the CBOR payload, the signature, and the public key.
- Implement `verify_manifest(signed: &SignedManifest) -> Result<Manifest, VerificationError>` which checks the signature against the embedded public key and deserializes on success.
- Define the on-disk format: a single file (`ironclad-manifest.signed`) containing a CBOR envelope with fields `version`, `public_key`, `signature`, `payload`.
- Write tests: sign and verify a manifest, tamper with one byte of the payload and verify rejection, tamper with the signature and verify rejection.

**Done when:** Manifests are signed, verifiable, and tamper-evident.

---

## 2.3 — Emitter Trait and Compiler Pipeline Integration

**Version: 0.2.0-dev**

Define the emitter abstraction and wire it into the compiler pipeline so adding new backends is mechanical.

**Deliverables:**

- Create an `ironclad-emit` crate.
- Define a trait:
  ```rust
  pub trait Emitter {
      type Output;
      fn emit(&self, manifest: &Manifest) -> Result<Self::Output, DiagnosticBag>;
  }
  ```
- Define an `EmitPlan` struct that holds a `Manifest` and a list of enabled emitters.
- Wire the CLI: `ironclad compile <file.ic>` now accepts an `--emit` flag (e.g. `--emit containerfile,kickstart`) and runs the selected emitters after resolution.
- Add an `--output-dir` flag specifying where emitted artifacts are written (default: `./build/`).
- With no emitters implemented yet, the pipeline writes only the signed manifest to the output directory.
- Write integration tests confirming the manifest is written to the output directory.

**Done when:** `ironclad compile example.ic --output-dir build/` produces `build/ironclad-manifest.signed`.

---

## 2.4 — Containerfile Emitter: Package Layer

**Version: 0.2.0-dev**

Begin the bootc Containerfile emitter with the most fundamental layer: base image selection and package installation.

**Deliverables:**

- Implement `ContainerfileEmitter` satisfying the `Emitter` trait. Output type is a `String` (the Containerfile content).
- Emit `FROM` directive based on the declared base image (e.g. `quay.io/fedora/fedora-bootc:41` or `quay.io/almalinux/almalinux-bootc:9`).
- Emit `RUN dnf install -y <packages>` from the declared package list, sorted alphabetically for reproducibility.
- Emit `RUN dnf remove -y <packages>` for any declared package removals.
- Emit `RUN dnf clean all` as a final layer.
- Write the Containerfile to `<output-dir>/Containerfile`.
- Write tests: a system declaring `packages: [vim, tmux, htop]` on a Fedora base produces the expected Containerfile content.

**Done when:** A Containerfile is generated that installs declared packages on the declared base.

---

## 2.5 — Containerfile Emitter: Users, Files, and Kernel Parameters

**Version: 0.2.0-dev**

Extend the Containerfile emitter with user account creation, file drops, and kernel parameter configuration.

**Deliverables:**

- Emit `RUN useradd` / `usermod` commands from declared user accounts (name, UID, GID, shell, groups, home directory, password hash handling).
- Emit `COPY` or heredoc `RUN cat <<EOF > /path` for declared file content (configuration files, scripts, etc.).
- Emit kernel command-line parameter configuration: write to `/etc/kernel/cmdline` for bootc-managed systems or the appropriate location for the declared bootloader.
- Emit timezone, locale, and hostname configuration from declared system properties.
- Emit `RUN systemctl enable <service>` or equivalent s6 enablement for declared services (service supervision tree details come later — this step handles basic enable/disable).
- Write tests for each emitted section in isolation and as a combined Containerfile.

**Done when:** The Containerfile covers packages, users, files, kernel parameters, and basic service enablement.

---

## 2.6 — Containerfile Emitter: Manifest Embedding and Final Assembly

**Version: 0.2.0-dev**

Embed the signed manifest into the image and finalize the Containerfile structure.

**Deliverables:**

- Emit a `COPY` directive that places `ironclad-manifest.signed` at a well-known path inside the image (e.g. `/etc/ironclad/manifest.signed`).
- Emit any required image labels (`LABEL ironclad.version=...`, `LABEL ironclad.manifest.hash=...`).
- Order all Containerfile directives for optimal layer caching: `FROM` → package operations → file drops → user creation → service enablement → manifest copy → labels.
- Add a `--dry-run` mode to the Containerfile emitter that prints the Containerfile to stdout without writing to disk.
- Write an end-to-end integration test: Ironclad source → resolved AST → manifest → Containerfile. Assert the Containerfile is syntactically valid and contains all expected directives.

**Done when:** The Containerfile emitter produces a complete, well-ordered Containerfile with an embedded manifest.

---

## 2.7 — Kickstart Emitter: Disk Partitioning and LVM

**Version: 0.2.0-dev**

Begin the Kickstart emitter with the part of the system that bootc cannot handle: disk layout.

**Deliverables:**

- Implement `KickstartEmitter` satisfying the `Emitter` trait. Output type is a `String` (the Kickstart file content).
- Emit `ignoredisk`, `clearpart`, `zerombr` directives from declared disk configuration.
- Emit `part` directives for: `/boot` (ext4), `/boot/efi` (EFI System Partition), and PV for LVM.
- Emit `volgroup` and `logvol` directives from declared LVM configuration (volume group name, logical volumes with sizes, mount points, filesystem types).
- Emit `bootloader` directive with declared kernel command-line parameters.
- Write tests: a system declaring a root LV, a home LV, and a swap LV on a single volume group produces the expected Kickstart partitioning section.

**Done when:** The Kickstart emitter generates correct disk partitioning and LVM directives.

---

## 2.8 — Kickstart Emitter: LUKS2 Encryption and TPM2/Clevis Binding

**Version: 0.2.0-dev**

Add encryption support to the Kickstart emitter.

**Deliverables:**

- Emit `part` and `logvol` directives with `--encrypted --luks-version=luks2` when LUKS2 is declared.
- Emit passphrase handling: `--passphrase` for initial provisioning (with a documented note that production deployments should use an enrollment workflow).
- Emit `%post` script commands for TPM2 binding via Clevis: `clevis luks bind -d <device> tpm2 '{"pcr_ids":"7"}'` (PCR set configurable from the declaration).
- Emit Tang server binding if declared: `clevis luks bind -d <device> tang '{"url":"<tang-url>"}'`.
- Emit Clevis dracut regeneration: `dracut -fv --regenerate-all` in `%post`.
- Write tests for: LUKS2 without TPM, LUKS2 with TPM2, LUKS2 with Tang, LUKS2 with both TPM2 and Tang (Shamir Secret Sharing via `clevis luks bind sss`).

**Done when:** The Kickstart emitter generates correct LUKS2 and Clevis binding configuration.

---

## 2.9 — Kickstart Emitter: Network, Users, and Final Assembly

**Version: 0.2.0-dev**

Complete the Kickstart emitter with remaining directives and the `%post` section.

**Deliverables:**

- Emit `network` directives: hostname, interface configuration (DHCP or static), VLAN, bonding if declared.
- Emit `rootpw`, `user`, and `sshkey` directives from declared user configuration.
- Emit `timezone`, `lang`, `keyboard` directives.
- Emit `ostreesetup` or `liveimg` directive pointing to the bootc-built OCI image (this is how Kickstart bootstraps a bootc-managed system).
- Assemble the final Kickstart file with correct section ordering: command section → `%packages` (minimal, since packages live in the image) → `%pre` (if needed) → `%post` (Clevis binding, any install-time-only commands) → `%end`.
- Write the Kickstart file to `<output-dir>/kickstart.ks`.
- Write an end-to-end test: Ironclad source with disk, encryption, network, and user declarations → Kickstart output. Validate structure and directive correctness.

**Done when:** The Kickstart emitter produces a complete `.ks` file covering disk layout, encryption, network, and user provisioning.

---

## 2.10 — Standard Class Library: Core Base Classes

**Version: 0.2.0-dev**

Ship the initial set of reusable classes written in Ironclad. These are the starting point for users and the proof that the class system works at scale.

**Deliverables:**

- Create a `stdlib/` directory in the repository.
- Write `HardenedRHELBase` class: SELinux enforcing, LUKS2 required, firewall enabled, password policy, audit logging enabled, unnecessary services disabled, RHEL/Alma/Fedora compatible package set.
- Write `FedoraBase` class extending `HardenedRHELBase`: Fedora-specific base image, Fedora-specific package names where they differ.
- Write `AlmaLinuxBase` class extending `HardenedRHELBase`: AlmaLinux-specific base image and packages.
- Write `S6ContainerHost` class: s6 as init system, container runtime packages, s6 service tree skeleton, overlay filesystem for containers.
- Write `SystemdServer` class: systemd as init system, standard server packages, journald configuration.
- Each class must compile, resolve, and produce correct Containerfile and Kickstart output.
- Write tests for each stdlib class: compile and assert the resolved output matches expected values.

**Done when:** Five stdlib classes compile and emit correct backend artifacts.

---

## 2.11 — Standard Class Library: SELinux MLS Profile

**Version: 0.2.0-dev**

Ship at least one SELinux MLS profile class. Full MLS policy generation is Phase 4, but the class library needs to declare the structure now so that the class system and emitters handle SELinux properties correctly.

**Deliverables:**

- Write `MLSWorkstation` class: declares `selinux_mls: enabled`, strictness level, user clearance ranges, default file labels, and network interface labels.
- Ensure the Containerfile emitter handles SELinux-related declarations: `RUN semanage` commands for custom mappings, SELinux mode configuration in `/etc/selinux/config`.
- Ensure the Kickstart emitter sets `selinux --enforcing` with the correct policy type.
- This class does not generate MLS policy files (that's Phase 4) — it declares the SELinux configuration that the generated policy will eventually enforce, and ensures the build artifacts correctly configure SELinux mode and basic type enforcement.
- Write tests: compile `MLSWorkstation`, verify Containerfile contains correct SELinux configuration.

**Done when:** An SELinux MLS profile class compiles and the emitters configure SELinux correctly in output artifacts.

---

## 2.12 — End-to-End Pipeline Validation

**Version: 0.2.0**

Prove the pipeline works by building a real system from Ironclad source.

**Deliverables:**

- Write a complete example system declaration (`examples/fedora-server.ic`) that extends `FedoraBase` with: custom disk layout (3 LVs + swap), LUKS2 with TPM2, two user accounts, a set of server packages, a static network configuration, SELinux enforcing.
- Compile to Containerfile + Kickstart + signed manifest.
- Document the manual validation steps in `docs/VALIDATION.md`: how to build the OCI image with `podman build`, how to boot an installer ISO with the generated Kickstart in a VM (libvirt/QEMU), and what to check on the installed system (partitions, encryption, users, packages, SELinux mode, manifest presence at `/etc/ironclad/manifest.signed`).
- If full automated VM testing is feasible (e.g. via `testcloud` or a CI VM), implement it. If not, document why and what manual steps are required.
- Fix any issues discovered during validation and add regression tests.

**Done when:** The example system compiles, the artifacts are valid, and the validation procedure is documented. Tag 0.2.0.

---

## 2.13 — Documentation and Release Polish

**Version: 0.2.1**

Document the backend architecture, update the class authoring guide, and make 0.2.x releasable.

**Deliverables:**

- Write `docs/EMITTERS.md`: architecture of the emitter system, how to add a new emitter, the manifest format specification.
- Write `docs/STDLIB.md`: catalog of standard library classes with descriptions, inheritance relationships, and overridable properties.
- Update `docs/LANGUAGE_GUIDE.md` with backend-related content: how `--emit` works, output directory structure, manifest signing.
- Update `ROADMAP.md` to mark Phase 2 complete.
- Review and clean up all compiler warnings, clippy lints, and TODO comments from Phase 2 development.
- Tag 0.2.1.

**Done when:** Documentation is complete, the codebase is clean, and 0.2.1 is tagged.

---

## Dependency Graph

```
Phase 1 complete (0.1.x)
 │
 ├── 2.1 (manifest data model)
 │    └── 2.2 (manifest signing)
 │         └── 2.3 (emitter trait + pipeline)
 │              ├── 2.4 (Containerfile: packages)
 │              │    └── 2.5 (Containerfile: users/files/kernel)
 │              │         └── 2.6 (Containerfile: manifest embed + assembly)
 │              │
 │              └── 2.7 (Kickstart: disk/LVM)
 │                   └── 2.8 (Kickstart: LUKS2/TPM2/Clevis)
 │                        └── 2.9 (Kickstart: network/users/assembly)
 │
 ├── 2.10 (stdlib: base classes)  ← can start once 2.6 + 2.9 land
 │    └── 2.11 (stdlib: MLS profile)
 │
 └── 2.12 (end-to-end validation) ← requires 2.6 + 2.9 + 2.10 + 2.11
      └── 2.13 (docs + release)
```

The Containerfile and Kickstart emitter tracks (2.4–2.6 and 2.7–2.9) can progress in parallel once the emitter trait is in place. The stdlib (2.10–2.11) requires both emitters to be functional for testing. End-to-end validation (2.12) is the integration point where everything must work together.
