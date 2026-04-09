# Ironclad SELinux Policy Syntax Specification

**Status:** Draft — syntax development, Phase 1  
**Scope:** System-level SELinux configuration, policy mode, security floor, user/role/type declarations, sensitivity and category bounds, module manifest, and file context declarations. This specification defines the SELinux parameters that the storage syntax consumes for compile-time context validation and that the compiler's SELinux targeted policy backend uses for policy generation.

---

## Design Principles

SELinux configuration in Ironclad exists at two levels. The first is the **system-level SELinux block** defined in this specification — it declares the policy mode, the security floor, the bounds of the MLS lattice, the SELinux users and their authorized roles and ranges, and the policy modules that define the type enforcement universe. The second level is the **per-mount and per-file labeling** declared in the storage syntax and the file declaration syntax, which consumes the parameters defined here for compile-time validation.

The compiler uses the system-level SELinux block as the ground truth for all SELinux validation across the entire source tree. A mount context referencing a type that does not exist in the module manifest is a compile-time error. A sensitivity range exceeding the declared `max_sensitivity` is a compile-time error. A user field referencing an undeclared SELinux user is a compile-time error. These guarantees are only as strong as the declarations in this block — the compiler validates against what is declared, not against what is installed on a running system.

---

## System-Level SELinux Block

Every Ironclad system declaration contains at most one `selinux` block. When omitted, the compiler applies defaults appropriate for RHEL-family targeted policy.

```
selinux {
    mode = enforcing
    policy = targeted
    floor = strict

    max_sensitivity = 15
    max_category = 1023

    user system_u {
        roles = [system_r, object_r, unconfined_r]
        range = s0-s15:c0.c1023
        default = true
    }

    user staff_u {
        roles = [staff_r, sysadm_r]
        range = s0-s15:c0.c1023
    }

    user user_u {
        roles = [user_r]
        range = s0
    }

    user unconfined_u {
        roles = [unconfined_r]
        range = s0-s15:c0.c1023
    }

    modules = [
        base,
        targeted,
        container,
        virt,
    ]

    booleans {
        container_manage_cgroup = true
        virt_use_nfs = false
        httpd_can_network_connect = true
    }
}
```

---

## Properties

### `mode`

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `mode` | `enforcing` \| `permissive` \| `disabled` | `enforcing` | SELinux enforcement mode. `enforcing` denies policy violations and logs them. `permissive` logs violations without denying. `disabled` turns SELinux off entirely. |

Under Ironclad's security floor, `disabled` is an error at `standard` and above. `permissive` is a warning at `standard`, an error at `strict` and above. The security floor is designed to prevent accidental deployment of systems with SELinux disabled.

**Compiler behavior:** Sets `SELINUX=<mode>` in `/etc/selinux/config`. When `mode = disabled`, the compiler skips all SELinux policy generation and context validation — no labels are emitted, no policy modules are compiled. When `mode = permissive`, the compiler generates policy and labels normally but sets the mode flag to permissive, allowing the system to boot and log denials for policy development.

---

### `policy`

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `policy` | `targeted` \| `mls` \| `minimum` | `targeted` | SELinux policy type. |

- **`targeted`** — The standard RHEL policy. Most processes run in the `unconfined_t` domain; targeted services are confined. Four-field contexts are used but sensitivity ranges are typically `s0` only.
- **`mls`** — Multi-Level Security policy. All processes are confined. Sensitivity levels and categories are actively enforced. Information flow between sensitivity levels follows Bell-LaPadula (no read up, no write down). This is the policy type required for defense and government environments handling classified data.
- **`minimum`** — A minimal policy confining only a small set of services. Not recommended for production.

**Compiler behavior:** Sets `SELINUXTYPE=<policy>` in `/etc/selinux/config`. The policy type determines which Reference Policy modules are available and which validation rules apply. Under `mls`, the compiler enforces that all mount contexts have valid sensitivity ranges and that all file contexts include the range field.

---

### `floor`

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `floor` | `baseline` \| `standard` \| `strict` \| `maximum` | `standard` | Security floor level. Controls how aggressively the compiler enforces SELinux-related requirements. |

The security floor is a cross-cutting concern that affects validation in the storage syntax, the file declaration syntax, and the SELinux block itself. Its interaction with storage is fully documented in [STORAGE_SYNTAX.md — Security Floor Validation](STORAGE_SYNTAX.md#security-floor-validation). Its interaction with SELinux declarations:

- **Baseline:** No enforcement. The operator's declarations are accepted as-is.
- **Standard:** `mode` must not be `disabled` (warning). All mount contexts should have valid four-field expressions (warnings for omissions under targeted; errors under MLS).
- **Strict:** `mode` must be `enforcing` (error for anything else). Under MLS, every mount must have an explicit context. SELinux booleans that weaken confinement (e.g., `allow_execheap`, `allow_execstack`) produce warnings.
- **Maximum:** Strict rules plus: under MLS, every file declaration and every mount must have an explicit context. `context=` on xattr-capable filesystems is an error (must use `defcontext`/`rootcontext`). Weakening booleans are errors, not warnings.

---

### `max_sensitivity`

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `max_sensitivity` | integer | `0` (targeted) / `15` (MLS) | Highest sensitivity level in the MLS lattice. Sensitivities range from `s0` through `s<max_sensitivity>`. |

Under `targeted` policy, sensitivity levels are technically present but almost always `s0`. The default of `0` reflects this — only `s0` is valid. Under `mls`, the RHEL default is `s0` through `s15`, corresponding to 16 sensitivity levels that map to classification levels (Unclassified through Top Secret with subdivisions).

**Validation:** The compiler uses this value to range-check every sensitivity reference in the entire source tree — mount contexts, file contexts, user range declarations, and SELinux policy module parameters. A sensitivity value exceeding `max_sensitivity` is a compile-time error.

---

### `max_category`

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `max_category` | integer | `1023` | Highest category number. Categories range from `c0` through `c<max_category>`. |

Categories provide compartmentalization within a sensitivity level. A system handling multiple independent projects at the same classification level uses categories to prevent cross-project data flow. The RHEL default of 1024 categories (`c0` through `c1023`) is sufficient for most deployments.

**Validation:** Every category reference in the source tree is range-checked against `max_category`. A category value exceeding this bound is a compile-time error.

---

## User Declarations

SELinux users are a namespace separate from Linux system users. An SELinux user is a policy identity that constrains which roles a Linux user can assume and, under MLS, which sensitivity range they can access. The `user` block inside the `selinux` block declares SELinux users for compile-time validation.

```
user staff_u {
    roles = [staff_r, sysadm_r]
    range = s0-s15:c0.c1023
}
```

**Properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `roles` | array of identifiers | required | Roles this user is authorized to assume. Referenced in context validation — the `role` field of a context expression must be in this list for the declared user. |
| `range` | MLS range expression | required under MLS | The user's authorized sensitivity-category range. Mount and file contexts referencing this user must have ranges that are dominated by (contained within) this range. |
| `default` | `true` \| `false` | `false` | Whether this is the default SELinux user for unmapped Linux users. At most one user may be marked default. |

**Validation:**
- Every role in the `roles` array must be a role that exists in the loaded policy modules.
- The `range` expression must be valid: sensitivities within `max_sensitivity`, categories within `max_category`, low ≤ high.
- At most one user may have `default = true`.
- `system_u` must be declared (it is required by all SELinux policies).

**Compiler behavior:** User declarations are cross-referenced during context validation. They also feed into the SELinux policy backend — the compiler generates `semanage login` mappings and, under MLS, user range constraints.

---

## Role Declarations

Roles are typically defined by the loaded policy modules rather than by Ironclad source. However, the compiler needs to know which roles exist for validation. Roles are inferred from two sources:

1. **Policy modules.** The loaded modules define roles as part of their type enforcement. The compiler reads the module manifest to extract role definitions.
2. **Explicit declaration.** When the compiler cannot introspect module contents (e.g., when modules are declared by name only without shipped policy source), roles can be explicitly declared.

```
selinux {
    role staff_r {
        types = [staff_t, user_home_t, user_tmp_t]
        dominates = [staff_r]
    }
}
```

Explicit role declarations are optional when the standard RHEL policy modules are loaded — the compiler has built-in knowledge of the standard role set (`system_r`, `object_r`, `unconfined_r`, `sysadm_r`, `staff_r`, `user_r`, `secadm_r`, `auditadm_r`, `dbadm_r`, `logadm_r`, `webadm_r`). Custom roles added by site-specific policy modules must be declared explicitly.

---

## Module Manifest

The `modules` property lists the SELinux policy modules that will be loaded in the built image. The compiler uses this list to determine which types, roles, and interfaces are available for validation and policy generation.

```
selinux {
    modules = [
        base,
        targeted,
        container,
        virt,
        httpd,
        postgresql,
        ssh,
    ]
}
```

Each identifier corresponds to a Reference Policy module name. The compiler resolves these against the Reference Policy module database shipped with the target distribution (e.g., `selinux-policy-targeted` on Fedora/RHEL). The module manifest determines:

- **Available types.** The `type` field of any context expression must reference a type defined by one of the loaded modules. Unknown types are compile-time errors.
- **Available roles.** Roles defined in the loaded modules are valid for use in context expressions and user declarations.
- **Available interfaces.** When the SELinux policy backend generates custom modules for declared services, it calls interfaces from the loaded modules. Missing interfaces produce compile-time errors with suggestions for which module to add.

**Compiler behavior:** The module list feeds into the SELinux policy backend's module loading order and into the `semodule` invocations emitted during image build. Modules are loaded in the declared order, with `base` always first.

---

## Booleans

SELinux booleans are runtime-tunable policy switches. The `booleans` block declares which booleans to set and their values.

```
selinux {
    booleans {
        container_manage_cgroup = true
        virt_use_nfs = false
        httpd_can_network_connect = true
        allow_execheap = false
        allow_execstack = false
    }
}
```

Each key is a boolean name; the value is `true` or `false`.

**Validation:**
- Boolean names must exist in the loaded policy modules. Unknown booleans are compile-time errors.
- Under `strict` security floor: booleans known to weaken confinement (`allow_execheap`, `allow_execstack`, `allow_execmem`, `selinuxuser_execmod`, `secure_mode_insmod`) set to `true` produce warnings.
- Under `maximum` security floor: weakening booleans set to `true` are errors.

**Compiler behavior:** Emits `setsebool -P <name> <value>` during image build for each declared boolean.

---

## MLS Range Expression Syntax

MLS range expressions appear in user declarations, mount context expressions (see [STORAGE_SYNTAX.md](STORAGE_SYNTAX.md)), and file context declarations. The syntax is shared across all contexts.

### Format

```
sensitivity(-sensitivity)?(:category_set)?
```

### Components

**Sensitivity:** `s<N>` where `N` is an integer from `0` through `max_sensitivity`.

```
s0          # lowest sensitivity
s15         # highest (with max_sensitivity = 15)
```

**Sensitivity range:** `s<low>-s<high>` where `low ≤ high`.

```
s0-s15      # full range
s4-s8       # partial range (e.g., Confidential through Secret)
s0-s0       # single level (equivalent to s0)
```

**Category set:** Follows a colon after the sensitivity. Categories are `c<N>` with `N` from `0` through `max_category`. Discrete categories are comma-separated. Ranges use dot notation.

```
s0:c0.c1023             # all categories at s0
s0-s15:c0.c1023         # all categories across full sensitivity range
s4:c0,c5,c12            # discrete categories
s0-s3:c0.c127,c256      # range plus discrete
```

### Dominance

MLS range expressions follow dominance rules:

- Range A **dominates** range B if A's low sensitivity is ≤ B's low sensitivity, A's high sensitivity is ≥ B's high sensitivity, and A's category set is a superset of B's category set.
- A user's authorized range must dominate all ranges used in contexts referencing that user.
- A mount context's range must be dominated by the user's range in the context expression.

The compiler validates dominance relationships at compile time.

### Classification Mapping

Ironclad does not impose a mapping between sensitivity levels and real-world classifications. However, the conventional RHEL MLS mapping is:

| Sensitivity | Classification |
| --- | --- |
| `s0` | Unclassified |
| `s1` – `s3` | (site-defined) |
| `s4` | Confidential |
| `s5` – `s7` | (site-defined) |
| `s8` | Secret |
| `s9` – `s11` | (site-defined) |
| `s12` | Top Secret |
| `s13` – `s15` | (site-defined / compartmented) |

Organizations define their own mapping. The compiler does not enforce or assume any particular mapping — it only validates structural correctness (ranges within bounds, dominance relationships).

---

## SELinux Context Expression Syntax

Context expressions appear in mount declarations, file declarations, and user declarations. The syntax is the same everywhere.

### Four-Field Format (Required)

```
user:role:type:range
```

All four fields are always required in Ironclad source. The three-field shorthand (`user:role:type`) accepted by some SELinux tools is not valid — under MLS, a missing range is a policy breach, and under targeted policy, the range is `s0` which should be explicit.

### Field Validation

| Field | Validated against | Error condition |
| --- | --- | --- |
| `user` | Declared `user` blocks in the `selinux` block | User not declared |
| `role` | `roles` array of the referenced user | Role not authorized for user |
| `type` | Types defined in loaded policy modules | Type not defined in module manifest |
| `range` | `max_sensitivity`, `max_category`, user's authorized range | Out of bounds; exceeds user range; inverted sensitivity |

### Examples

```
system_u:object_r:boot_t:s0
system_u:object_r:var_t:s0-s15:c0.c1023
staff_u:staff_r:staff_home_t:s4:c0.c255
system_u:object_r:container_var_lib_t:s0
system_u:object_r:classified_content_t:s8:c0.c127
```

---

## Interaction with Storage Syntax

The SELinux system-level block is the authority for all storage-related SELinux validation documented in [STORAGE_SYNTAX.md — SELinux Context on Mount Expressions](STORAGE_SYNTAX.md#selinux-context-on-mount-expressions) and [STORAGE_SYNTAX.md — SELinux Sensitivity and Category Validation](STORAGE_SYNTAX.md#selinux-sensitivity-and-category-validation). Specifically:

- `max_sensitivity` and `max_category` bound all range expressions in mount contexts.
- `user` declarations validate the `user` and `role` fields of mount contexts.
- `modules` determines which types are valid in the `type` field.
- `policy` determines whether MLS-specific validation rules apply (four-field requirement, range dominance, xattr-incapable filesystem context enforcement).
- `floor` controls the severity of missing-context diagnostics (warning vs. error).

The storage compiler does not generate SELinux policy — it only consumes the declarations here for validation. Policy generation is the responsibility of the SELinux targeted policy backend (Phase 4), which uses both this block and the fully resolved system AST.

---

## Interaction with File Declarations

File and directory declarations (specified separately) carry SELinux labels as file context expressions:

```
file /etc/myapp/config.conf {
    owner = root
    group = myapp
    mode = 0640
    context = system_u:object_r:myapp_conf_t:s0
    content = template("myapp.conf.j2")
}
```

The `context` field on file declarations is validated identically to mount contexts — same four-field requirement, same cross-referencing against users, roles, types, and range bounds.

The compiler accumulates all file context declarations and, during the SELinux policy backend pass (Phase 4), generates `.fc` file context specification files that `restorecon` uses to label the filesystem. This closes the loop: the declared context on a file is both validated at compile time and enforced at build time by the generated policy.

---

## Defaults

When the `selinux` block is omitted entirely, the compiler applies these defaults:

| Property | Default |
| --- | --- |
| `mode` | `enforcing` |
| `policy` | `targeted` |
| `floor` | `standard` |
| `max_sensitivity` | `0` |
| `max_category` | `1023` |
| Users | `system_u`, `unconfined_u`, `root` (standard RHEL targeted set) |
| Modules | `base`, `targeted` |
| Booleans | Distribution defaults |

Under targeted policy with default `max_sensitivity = 0`, only `s0` is a valid sensitivity. This means mount contexts like `system_u:object_r:boot_t:s0` pass validation, while `s0-s15` would fail. To use MLS ranges, either set `policy = mls` (which changes `max_sensitivity` default to `15`) or explicitly set `max_sensitivity` to the desired value.

---

## Compiler Validation Summary

All SELinux validation is performed at compile time. No validation depends on a running system or installed policy.

### Errors (halt compilation)

- `mode = disabled` at `standard` floor or above.
- `mode = permissive` at `strict` floor or above.
- Sensitivity value exceeding `max_sensitivity` anywhere in the source tree.
- Category value exceeding `max_category` anywhere in the source tree.
- Inverted sensitivity range (`s<high>-s<low>` where high < low).
- Context `user` field referencing an undeclared SELinux user.
- Context `role` field not authorized for the declared user.
- Context `type` field not defined in any loaded policy module.
- Context range exceeding the declared user's authorized range.
- Boolean name not defined in loaded policy modules.
- Weakening booleans set to `true` at `maximum` floor.
- Missing context on non-xattr filesystem mount at `strict` floor or above under MLS.
- `context=` on xattr-capable filesystem at `maximum` floor.
- More than one user with `default = true`.
- `system_u` not declared.
- Three-field context expression (missing range).

### Warnings

- `mode = permissive` at `standard` floor.
- Missing context on mount at `standard` floor under MLS.
- `context=` on xattr-capable filesystem at floors below `maximum`.
- Weakening booleans set to `true` at `strict` floor.
- Duplicate boolean declarations (last wins, but warn about the override).

---

## Reserved Keywords

The following words are reserved in SELinux context and cannot be used as block names or identifiers within the `selinux` block:

`selinux`, `mode`, `policy`, `floor`, `user`, `role`, `booleans`, `modules`, `enforcing`, `permissive`, `disabled`, `targeted`, `mls`, `minimum`, `baseline`, `standard`, `strict`, `maximum`, `range`, `roles`, `types`, `dominates`, `default`, `true`, `false`

---

## What This Document Does Not Cover

- **Type enforcement rule authoring.** Writing custom `.te` policy modules by hand. Ironclad supports declaring hand-authored policy files through file primitives; the compiler backend can also generate policy from the system topology (Phase 4). Neither is defined here.
- **SELinux policy module internals.** The structure of `.te`, `.fc`, and `.if` files. The compiler generates these; the operator does not need to understand their internals unless authoring custom policy.
- **Runtime policy management.** `semanage`, `setsebool`, `restorecon`, `audit2allow`. The compiler emits the equivalent operations during image build; runtime policy management is outside Ironclad's scope.
- **Conditional policy and type transitions.** Advanced policy constructs like `type_transition`, `allow`, `dontaudit`, and conditional booleans in TE rules. These are internal to policy modules, not exposed in the Ironclad language.
- **Network labeling.** CIPSO, NetLabel, labeled IPsec. Network-level SELinux labeling is a separate domain from the file and mount labeling defined in the storage and file syntaxes.
