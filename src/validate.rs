use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::errors::{Diagnostic, IroncladError, Result, Severity};

/// Run all structural validation passes on a parsed storage AST.
/// Returns Ok(warnings) on success, or Err with all errors collected.
pub fn validate(file: &StorageFile) -> Result<Vec<Diagnostic>> {
    let mut ctx = ValidationContext::default();

    for decl in &file.declarations {
        match decl {
            StorageDecl::Disk(disk) => validate_disk(&mut ctx, disk),
            StorageDecl::MdRaid(md) => validate_mdraid(&mut ctx, md),
        }
    }

    if ctx.errors.is_empty() {
        Ok(ctx.warnings)
    } else {
        Err(IroncladError::ValidationError {
            errors: ctx.errors,
        })
    }
}

#[derive(Default)]
struct ValidationContext {
    errors: Vec<Diagnostic>,
    warnings: Vec<Diagnostic>,
    /// All mount target paths seen, for uniqueness check
    mount_targets: HashMap<String, Span>,
    /// All mdraid member disks, for uniqueness check
    mdraid_members: HashSet<String>,
    /// All block names, for uniqueness check
    block_names: HashMap<String, Span>,
}

impl ValidationContext {
    fn error(&mut self, message: String, span: Option<Span>, block_name: Option<String>, hint: Option<String>) {
        self.errors.push(Diagnostic {
            severity: Severity::Error,
            message,
            span,
            block_name,
            hint,
        });
    }

    fn warning(&mut self, message: String, span: Option<Span>, block_name: Option<String>, hint: Option<String>) {
        self.warnings.push(Diagnostic {
            severity: Severity::Warning,
            message,
            span,
            block_name,
            hint,
        });
    }

    fn check_mount_unique(&mut self, target: &str, span: &Span, block_name: &str) {
        if let Some(prev) = self.mount_targets.get(target) {
            self.error(
                format!("duplicate mount target `{target}` (previously declared at line {})", prev.line),
                Some(span.clone()),
                Some(block_name.to_string()),
                Some("every mount target path must be unique across the entire source tree".to_string()),
            );
        } else {
            self.mount_targets.insert(target.to_string(), span.clone());
        }
    }

    fn check_block_name(&mut self, name: &str, span: &Span) {
        if RESERVED_KEYWORDS.contains(&name) {
            self.error(
                format!("`{name}` is a reserved keyword and cannot be used as a block name"),
                Some(span.clone()),
                None,
                None,
            );
        }
        if let Some(prev) = self.block_names.get(name) {
            self.warning(
                format!("block name `{name}` already used at line {}", prev.line),
                Some(span.clone()),
                None,
                Some("block names should be unique for clear reference".to_string()),
            );
        } else {
            self.block_names.insert(name.to_string(), span.clone());
        }
    }
}

const RESERVED_KEYWORDS: &[&str] = &[
    "disk", "mdraid", "luks2", "luks1", "lvm", "thin", "ext4", "xfs", "btrfs",
    "fat32", "swap", "ntfs", "raw", "subvol", "mount", "remaining", "none",
    "whole", "true", "false", "context", "fscontext", "defcontext", "rootcontext",
];

// ─── Disk Validation ─────────────────────────────────────────

fn validate_disk(ctx: &mut ValidationContext, disk: &DiskBlock) {
    let label = get_property_str(&disk.properties, "label");

    // Check: label is required
    if label.is_none() {
        ctx.error(
            "disk block requires a `label` property (gpt, msdos, or none)".to_string(),
            Some(disk.span.clone()),
            Some(disk.device.clone()),
            None,
        );
    }

    let is_whole_disk = label.as_deref() == Some("none");

    if is_whole_disk {
        // label = none: exactly one filesystem child, no partition properties
        if disk.children.len() != 1 {
            ctx.error(
                format!(
                    "disk with `label = none` must have exactly one child, found {}",
                    disk.children.len()
                ),
                Some(disk.span.clone()),
                Some(disk.device.clone()),
                Some("whole-disk devices use a single filesystem or luks block".to_string()),
            );
        }
        // Children should not have index/size/start/end/type
        for child in &disk.children {
            check_no_partition_props(ctx, child, &disk.device);
        }
    } else {
        // Partitioned disk validation
        validate_partition_children(ctx, &disk.children, &disk.device);
    }

    // Validate children recursively
    for child in &disk.children {
        validate_partition_child(ctx, child);
    }
}

fn check_no_partition_props(ctx: &mut ValidationContext, child: &PartitionChild, disk_name: &str) {
    let props = match child {
        PartitionChild::Filesystem(f) => &f.properties,
        PartitionChild::Luks(l) => &l.properties,
        PartitionChild::Lvm(l) => &l.properties,
        PartitionChild::Raw(r) => &r.properties,
        PartitionChild::Swap(s) => &s.properties,
    };

    for prop in props {
        if ["index", "size", "start", "end", "type"].contains(&prop.key.as_str()) {
            ctx.error(
                format!(
                    "property `{}` is not valid inside a whole-disk device (label = none)",
                    prop.key
                ),
                Some(prop.span.clone()),
                Some(disk_name.to_string()),
                Some("the block device is the filesystem's backing device directly".to_string()),
            );
        }
    }
}

fn validate_partition_children(
    ctx: &mut ValidationContext,
    children: &[PartitionChild],
    parent_name: &str,
) {
    let mut indices: HashMap<i64, Span> = HashMap::new();
    let mut remaining_count = 0;

    for child in children {
        let (props, span) = match child {
            PartitionChild::Filesystem(f) => (&f.properties, &f.span),
            PartitionChild::Luks(l) => (&l.properties, &l.span),
            PartitionChild::Lvm(l) => (&l.properties, &l.span),
            PartitionChild::Raw(r) => (&r.properties, &r.span),
            PartitionChild::Swap(s) => (&s.properties, &s.span),
        };

        // Check index uniqueness
        if let Some(idx) = get_property_int(props, "index") {
            if idx <= 0 {
                ctx.error(
                    format!("partition index must be positive, got {idx}"),
                    Some(span.clone()),
                    Some(parent_name.to_string()),
                    None,
                );
            }
            if let Some(prev) = indices.get(&idx) {
                ctx.error(
                    format!("duplicate partition index {idx} (previously at line {})", prev.line),
                    Some(span.clone()),
                    Some(parent_name.to_string()),
                    None,
                );
            } else {
                indices.insert(idx, span.clone());
            }
        }

        // Check `remaining` count
        if get_property_remaining(props) {
            remaining_count += 1;
        }
    }

    if remaining_count > 1 {
        ctx.error(
            format!("only one `remaining` size is permitted per parent scope, found {remaining_count}"),
            None,
            Some(parent_name.to_string()),
            None,
        );
    }
}

// ─── mdraid Validation ───────────────────────────────────────

fn validate_mdraid(ctx: &mut ValidationContext, md: &MdRaidBlock) {
    ctx.check_block_name(&md.name, &md.span);

    // Required: level
    if get_property_str(&md.properties, "level").is_none()
        && get_property_int(&md.properties, "level").is_none()
    {
        ctx.error(
            "mdraid block requires a `level` property".to_string(),
            Some(md.span.clone()),
            Some(md.name.clone()),
            Some("valid levels: 0, 1, 5, 6, 10".to_string()),
        );
    }

    // Required: disks
    if !md.properties.iter().any(|p| p.key == "disks") {
        ctx.error(
            "mdraid block requires a `disks` property".to_string(),
            Some(md.span.clone()),
            Some(md.name.clone()),
            None,
        );
    }

    // Check member disk uniqueness across all arrays
    for prop in &md.properties {
        if prop.key == "disks" {
            if let Value::Array(items) = &prop.value {
                for item in items {
                    if let Value::DevicePath(path) = item {
                        if !ctx.mdraid_members.insert(path.clone()) {
                            ctx.error(
                                format!("device `{path}` is already a member of another mdraid array"),
                                Some(prop.span.clone()),
                                Some(md.name.clone()),
                                None,
                            );
                        }
                    }
                }
            }
        }
    }

    for child in &md.children {
        validate_partition_child(ctx, child);
    }
}

// ─── Recursive Child Validation ──────────────────────────────

fn validate_partition_child(ctx: &mut ValidationContext, child: &PartitionChild) {
    match child {
        PartitionChild::Filesystem(f) => validate_fs(ctx, f),
        PartitionChild::Luks(l) => validate_luks(ctx, l),
        PartitionChild::Lvm(l) => validate_lvm(ctx, l),
        PartitionChild::Raw(r) => {
            ctx.check_block_name(&r.name, &r.span);
        }
        PartitionChild::Swap(s) => {
            ctx.check_block_name(&s.name, &s.span);
        }
    }
}

fn validate_fs(ctx: &mut ValidationContext, fs: &FsBlock) {
    ctx.check_block_name(&fs.name, &fs.span);

    // Check mount target from inline mount property
    for prop in &fs.properties {
        if prop.key == "mount" {
            if let Value::Mount(ref mount) = prop.value {
                ctx.check_mount_unique(&mount.target, &fs.span, &fs.name);
                validate_mount_selinux_inline(ctx, mount, fs.fs_type, &fs.name, &fs.span);
            }
        }
    }

    // Check mount target from extended mount block
    if let Some(ref mb) = fs.mount_block {
        if let Some(ref target) = mb.target {
            ctx.check_mount_unique(target, &mb.span, &fs.name);
        }
        validate_mount_selinux_ext(ctx, mb, fs.fs_type, &fs.name);
    }

    // subvol only valid inside btrfs
    if !fs.subvolumes.is_empty() && fs.fs_type != FsType::Btrfs {
        ctx.error(
            format!("subvolume blocks are only valid inside btrfs, not {}", fs.fs_type),
            Some(fs.span.clone()),
            Some(fs.name.clone()),
            None,
        );
    }

    for sv in &fs.subvolumes {
        validate_subvol(ctx, sv, fs.fs_type);
    }
}

fn validate_subvol(ctx: &mut ValidationContext, sv: &SubvolBlock, parent_fs: FsType) {
    // Check mount
    for prop in &sv.properties {
        if prop.key == "mount" {
            if let Value::Mount(ref mount) = prop.value {
                ctx.check_mount_unique(&mount.target, &sv.span, &sv.name);
            }
        }
    }

    if let Some(ref mb) = sv.mount_block {
        if let Some(ref target) = mb.target {
            ctx.check_mount_unique(target, &mb.span, &sv.name);
        }
        validate_mount_selinux_ext(ctx, mb, parent_fs, &sv.name);
    }
}

fn validate_luks(ctx: &mut ValidationContext, luks: &LuksBlock) {
    ctx.check_block_name(&luks.name, &luks.span);

    // LUKS without LVM child can have at most one filesystem
    let fs_count = luks
        .children
        .iter()
        .filter(|c| matches!(c, LuksChild::Filesystem(_)))
        .count();
    let lvm_count = luks
        .children
        .iter()
        .filter(|c| matches!(c, LuksChild::Lvm(_)))
        .count();

    if lvm_count == 0 && fs_count > 1 {
        ctx.error(
            format!(
                "luks block without lvm may contain at most one filesystem child, found {fs_count}"
            ),
            Some(luks.span.clone()),
            Some(luks.name.clone()),
            Some("LUKS opens to a single block device".to_string()),
        );
    }

    for child in &luks.children {
        match child {
            LuksChild::Filesystem(f) => validate_fs(ctx, f),
            LuksChild::Lvm(l) => validate_lvm(ctx, l),
            LuksChild::Swap(s) => {
                ctx.check_block_name(&s.name, &s.span);
            }
        }
    }
}

fn validate_lvm(ctx: &mut ValidationContext, lvm: &LvmBlock) {
    ctx.check_block_name(&lvm.name, &lvm.span);

    let mut remaining_count = 0;

    for child in &lvm.children {
        match child {
            LvmChild::Filesystem(f) => {
                validate_fs(ctx, f);
                if get_property_remaining(&f.properties) {
                    remaining_count += 1;
                }
            }
            LvmChild::Swap(s) => {
                ctx.check_block_name(&s.name, &s.span);
                if get_property_remaining(&s.properties) {
                    remaining_count += 1;
                }
            }
            LvmChild::Thin(t) => {
                validate_thin(ctx, t, &lvm.name);
                if get_property_remaining(&t.properties) {
                    remaining_count += 1;
                }
            }
        }
    }

    if remaining_count > 1 {
        ctx.error(
            format!("only one `remaining` size is permitted per lvm scope, found {remaining_count}"),
            Some(lvm.span.clone()),
            Some(lvm.name.clone()),
            None,
        );
    }
}

fn validate_thin(ctx: &mut ValidationContext, thin: &ThinBlock, vg_name: &str) {
    ctx.check_block_name(&thin.name, &thin.span);

    // thin pools require a size
    if !thin.properties.iter().any(|p| p.key == "size") {
        ctx.error(
            "thin pool requires a `size` property".to_string(),
            Some(thin.span.clone()),
            Some(thin.name.clone()),
            None,
        );
    }

    // Check overcommit
    let pool_size = get_property_size_bytes(&thin.properties, "size");
    if let Some(pool_bytes) = pool_size {
        let total_virtual: u64 = thin
            .children
            .iter()
            .filter_map(|c| match c {
                ThinChild::Filesystem(f) => get_property_size_bytes(&f.properties, "size"),
                ThinChild::Swap(s) => get_property_size_bytes(&s.properties, "size"),
            })
            .sum();

        let warn_pct = get_property_pct(&thin.properties, "overcommit_warn").unwrap_or(80);
        let deny_pct = get_property_pct(&thin.properties, "overcommit_deny");

        if pool_bytes > 0 {
            let usage_pct = (total_virtual * 100) / pool_bytes;
            if let Some(deny) = deny_pct {
                if usage_pct > deny {
                    ctx.error(
                        format!(
                            "thin pool `{}` virtual allocation ({usage_pct}%) exceeds overcommit_deny threshold ({deny}%)",
                            thin.name
                        ),
                        Some(thin.span.clone()),
                        Some(vg_name.to_string()),
                        None,
                    );
                }
            }
            if usage_pct > warn_pct {
                ctx.warning(
                    format!(
                        "thin pool `{}` virtual allocation ({usage_pct}%) exceeds overcommit_warn threshold ({warn_pct}%)",
                        thin.name
                    ),
                    Some(thin.span.clone()),
                    Some(vg_name.to_string()),
                    None,
                );
            }
        }
    }

    for child in &thin.children {
        match child {
            ThinChild::Filesystem(f) => validate_fs(ctx, f),
            ThinChild::Swap(s) => {
                ctx.check_block_name(&s.name, &s.span);
            }
        }
    }
}

// ─── SELinux Mount Validation ────────────────────────────────

fn validate_mount_selinux_inline(
    ctx: &mut ValidationContext,
    mount: &MountExpr,
    fs_type: FsType,
    block_name: &str,
    span: &Span,
) {
    if let Some(ref _context) = mount.context {
        // Using context= on xattr-capable filesystem is a warning
        if fs_type.supports_xattr() {
            ctx.warning(
                format!(
                    "`context` on xattr-capable filesystem {} silently overrides all xattr labels",
                    fs_type
                ),
                Some(span.clone()),
                Some(block_name.to_string()),
                Some("consider using `defcontext` or `rootcontext` in an extended mount block instead".to_string()),
            );
        }
    }
}

fn validate_mount_selinux_ext(
    ctx: &mut ValidationContext,
    mount: &MountBlockExt,
    fs_type: FsType,
    block_name: &str,
) {
    // context is mutually exclusive with fscontext/defcontext/rootcontext
    if mount.context.is_some() {
        let has_others = mount.fscontext.is_some()
            || mount.defcontext.is_some()
            || mount.rootcontext.is_some();
        if has_others {
            ctx.error(
                "`context` is mutually exclusive with `fscontext`, `defcontext`, and `rootcontext`".to_string(),
                Some(mount.span.clone()),
                Some(block_name.to_string()),
                Some(
                    "`context` overrides all xattr labels; the other three are for per-file labeling".to_string(),
                ),
            );
        }

        // context on xattr-capable is a warning
        if fs_type.supports_xattr() {
            ctx.warning(
                format!(
                    "`context` on xattr-capable filesystem {} silently overrides all xattr labels",
                    fs_type
                ),
                Some(mount.span.clone()),
                Some(block_name.to_string()),
                Some("consider using `defcontext` or `rootcontext` instead".to_string()),
            );
        }
    }
}

// ─── Property Helpers ────────────────────────────────────────

fn get_property_str<'a>(props: &'a [Property], key: &str) -> Option<&'a str> {
    props.iter().find(|p| p.key == key).and_then(|p| match &p.value {
        Value::Ident(s) | Value::String(s) => Some(s.as_str()),
        _ => None,
    })
}

fn get_property_int(props: &[Property], key: &str) -> Option<i64> {
    props.iter().find(|p| p.key == key).and_then(|p| match &p.value {
        Value::Integer(n) => Some(*n),
        Value::Ident(s) => s.parse().ok(),
        _ => None,
    })
}

fn get_property_remaining(props: &[Property]) -> bool {
    props.iter().any(|p| p.key == "size" && matches!(&p.value, Value::Remaining))
}

fn get_property_size_bytes(props: &[Property], key: &str) -> Option<u64> {
    props.iter().find(|p| p.key == key).and_then(|p| match &p.value {
        Value::Size(sv) => Some(sv.to_bytes()),
        _ => None,
    })
}

fn get_property_pct(props: &[Property], key: &str) -> Option<u64> {
    props.iter().find(|p| p.key == key).and_then(|p| match &p.value {
        Value::Percentage(n) => Some(*n),
        _ => None,
    })
}
