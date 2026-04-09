use crate::ast::*;
use crate::parser::parse_storage;
use crate::validate::validate;

// ─── Basic Parsing Tests ─────────────────────────────────────

#[test]
fn parse_simple_disk() {
    let input = r#"
disk /dev/sda {
    label = gpt
}
"#;
    let ast = parse_storage(input).expect("should parse");
    assert_eq!(ast.declarations.len(), 1);
    match &ast.declarations[0] {
        StorageDecl::Disk(d) => {
            assert_eq!(d.device, "/dev/sda");
            assert_eq!(d.properties.len(), 1);
            assert_eq!(d.properties[0].key, "label");
        }
        _ => panic!("expected disk"),
    }
}

#[test]
fn parse_disk_with_partitions() {
    let input = r#"
disk /dev/sda {
    label = gpt

    fat32 efi {
        index = 1
        size = 1G
        type = ef00
        mount = /boot/efi [nodev, nosuid, noexec]
    }

    ext4 boot {
        index = 2
        size = 1G
        mount = /boot [nodev, nosuid, noexec]
    }

    luks2 system {
        index = 3
        size = remaining
        cipher = aes-xts-plain64
        key_size = 512

        lvm vg0 {
            ext4 root { size = 50G; mount = / }
            swap swap0 { size = 16G }

            thin pool0 {
                size = 200G

                xfs data { size = 300G; mount = /srv }
            }
        }
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    assert_eq!(ast.declarations.len(), 1);

    if let StorageDecl::Disk(d) = &ast.declarations[0] {
        assert_eq!(d.children.len(), 3);

        // EFI partition
        if let PartitionChild::Filesystem(f) = &d.children[0] {
            assert_eq!(f.fs_type, FsType::Fat32);
            assert_eq!(f.name, "efi");
        } else {
            panic!("expected fat32");
        }

        // Boot partition
        if let PartitionChild::Filesystem(f) = &d.children[1] {
            assert_eq!(f.fs_type, FsType::Ext4);
            assert_eq!(f.name, "boot");
        } else {
            panic!("expected ext4");
        }

        // LUKS partition with LVM
        if let PartitionChild::Luks(l) = &d.children[2] {
            assert_eq!(l.version, LuksVersion::Luks2);
            assert_eq!(l.name, "system");
            assert_eq!(l.children.len(), 1);

            if let LuksChild::Lvm(lvm) = &l.children[0] {
                assert_eq!(lvm.name, "vg0");
                assert_eq!(lvm.children.len(), 3); // root, swap, thin
            } else {
                panic!("expected lvm inside luks");
            }
        } else {
            panic!("expected luks2");
        }
    } else {
        panic!("expected disk");
    }
}

#[test]
fn parse_btrfs_with_subvolumes() {
    let input = r#"
disk /dev/nvme0n1 {
    label = gpt

    btrfs root {
        index = 1
        size = remaining
        compress = zstd:1

        subvol @ {
            mount = /
        }

        subvol @home {
            mount = /home [nodev, nosuid, compress=zstd:3]
            quota = 50G
        }

        subvol @var_log {
            mount = /var/log [nodev, nosuid, noexec]
            quota = 10G
        }
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    if let StorageDecl::Disk(d) = &ast.declarations[0] {
        if let PartitionChild::Filesystem(f) = &d.children[0] {
            assert_eq!(f.fs_type, FsType::Btrfs);
            assert_eq!(f.subvolumes.len(), 3);
            assert_eq!(f.subvolumes[0].name, "@");
            assert_eq!(f.subvolumes[1].name, "@home");
            assert_eq!(f.subvolumes[2].name, "@var_log");
        } else {
            panic!("expected btrfs");
        }
    }
}

#[test]
fn parse_mdraid() {
    let input = r#"
mdraid md0 {
    level = 10
    disks = [/dev/sda, /dev/sdb, /dev/sdc, /dev/sdd]
    chunk = 512K

    luks2 encrypted_array {
        cipher = aes-xts-plain64
        key_size = 512
        tpm2 = true

        lvm vg_data {
            xfs databases {
                size = 500G
                mount = /var/lib/postgres [nodev, nosuid]
            }
        }
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    assert_eq!(ast.declarations.len(), 1);

    if let StorageDecl::MdRaid(md) = &ast.declarations[0] {
        assert_eq!(md.name, "md0");
        assert_eq!(md.children.len(), 1);
    }
}

#[test]
fn parse_whole_disk() {
    let input = r#"
disk /dev/sdc {
    label = none

    xfs scratch {
        mount = /mnt/scratch
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    let warnings = validate(&ast).expect("should validate");
    assert!(warnings.is_empty());
}

#[test]
fn parse_mount_with_selinux_context() {
    let input = r#"
disk /dev/sda {
    label = gpt

    fat32 efi {
        index = 1
        size = 1G
        type = ef00
        mount = /boot/efi [nodev, nosuid, noexec] context system_u:object_r:boot_t:s0
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    if let StorageDecl::Disk(d) = &ast.declarations[0] {
        if let PartitionChild::Filesystem(f) = &d.children[0] {
            let mount_prop = f.properties.iter().find(|p| p.key == "mount").unwrap();
            if let Value::Mount(ref m) = mount_prop.value {
                assert_eq!(m.target, "/boot/efi");
                assert_eq!(m.options.len(), 3);
                assert!(m.context.is_some());
                let ctx = m.context.as_ref().unwrap();
                assert_eq!(ctx.user, "system_u");
                assert_eq!(ctx.role, "object_r");
                assert_eq!(ctx.typ, "boot_t");
                assert_eq!(ctx.range.low.level, 0);
            } else {
                panic!("expected mount value");
            }
        }
    }
}

#[test]
fn parse_extended_mount_block() {
    let input = r#"
disk /dev/sda {
    label = gpt

    ext4 data {
        index = 1
        size = 500G

        mount {
            target = /srv/data
            options = [nodev, nosuid, noexec]
            automount = false
            timeout = 30
            requires = [network-online.target]
        }
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    if let StorageDecl::Disk(d) = &ast.declarations[0] {
        if let PartitionChild::Filesystem(f) = &d.children[0] {
            let mb = f.mount_block.as_ref().expect("should have mount block");
            assert_eq!(mb.target.as_deref(), Some("/srv/data"));
            assert_eq!(mb.options.len(), 3);
            assert_eq!(mb.automount, Some(false));
            assert_eq!(mb.timeout, Some(30));
            assert_eq!(mb.requires.len(), 1);
        }
    }
}

#[test]
fn parse_extended_mount_with_selinux() {
    let input = r#"
disk /dev/sda {
    label = gpt

    ext4 containers {
        index = 1
        size = 200G

        mount {
            target = /var/lib/containers
            options = [nodev, nosuid]
            defcontext = system_u:object_r:container_var_lib_t:s0
            rootcontext = system_u:object_r:container_var_lib_t:s0
        }
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    if let StorageDecl::Disk(d) = &ast.declarations[0] {
        if let PartitionChild::Filesystem(f) = &d.children[0] {
            let mb = f.mount_block.as_ref().unwrap();
            assert!(mb.defcontext.is_some());
            assert!(mb.rootcontext.is_some());
            assert!(mb.context.is_none());
        }
    }
}

#[test]
fn parse_mls_sensitivity_range() {
    let input = r#"
disk /dev/sda {
    label = gpt

    ext4 shared_data {
        index = 1
        size = 100G

        mount {
            target = /srv/shared
            options = [nodev, nosuid, noexec]
            defcontext = system_u:object_r:shared_content_t:s0-s9:c0.c255
        }
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    if let StorageDecl::Disk(d) = &ast.declarations[0] {
        if let PartitionChild::Filesystem(f) = &d.children[0] {
            let mb = f.mount_block.as_ref().unwrap();
            let ctx = mb.defcontext.as_ref().unwrap();
            assert_eq!(ctx.range.low.level, 0);
            assert_eq!(ctx.range.high.as_ref().unwrap().level, 9);
            assert_eq!(ctx.range.categories.as_deref(), Some("c0.c255"));
        }
    }
}

// ─── Multi-Disk Scenario from Spec ───────────────────────────

#[test]
fn parse_multi_disk_scenario() {
    let input = r#"
disk /dev/sda {
    label = gpt

    fat32 efi {
        index = 1
        size = 1G
        type = ef00
        mount = /boot/efi [nodev, nosuid, noexec] context system_u:object_r:boot_t:s0
    }

    ext4 boot {
        index = 2
        size = 1G
        mount = /boot [nodev, nosuid, noexec]
    }
}

disk /dev/nvme0n1 {
    label = gpt

    luks2 system {
        index = 1
        size = remaining
        tpm2 = true

        lvm vg_system {
            btrfs root {
                size = remaining
                compress = zstd:1

                subvol @ { mount = / }
                subvol @home { mount = /home [nodev, nosuid] }
                subvol @var { mount = /var [nodev, nosuid, noexec] }
                subvol @snapshots { mount = /.snapshots }
            }

            swap swap0 { size = 32G }
        }
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    assert_eq!(ast.declarations.len(), 2);

    // Validate it
    let warnings = validate(&ast).expect("should validate without errors");
    // May have warnings for context on fat32, that's fine
    let _ = warnings;
}

// ─── Validation Tests ────────────────────────────────────────

#[test]
fn validate_rejects_missing_disk_label() {
    let input = r#"
disk /dev/sda {
    ext4 root {
        size = 50G
        mount = /
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    let result = validate(&ast);
    assert!(result.is_err(), "should reject disk without label");
}

#[test]
fn validate_rejects_whole_disk_multiple_children() {
    let input = r#"
disk /dev/sda {
    label = none

    ext4 root { size = 50G; mount = / }
    swap swap0 { size = 16G }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    let result = validate(&ast);
    assert!(result.is_err(), "label=none with 2 children should fail");
}

#[test]
fn validate_rejects_duplicate_mount_targets() {
    let input = r#"
disk /dev/sda {
    label = gpt

    ext4 root1 {
        index = 1
        size = 50G
        mount = /data
    }

    ext4 root2 {
        index = 2
        size = 50G
        mount = /data
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    let result = validate(&ast);
    assert!(result.is_err(), "duplicate mount targets should fail");
}

#[test]
fn validate_rejects_duplicate_partition_index() {
    let input = r#"
disk /dev/sda {
    label = gpt

    ext4 part_a {
        index = 1
        size = 50G
        mount = /a
    }

    ext4 part_b {
        index = 1
        size = 50G
        mount = /b
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    let result = validate(&ast);
    assert!(result.is_err(), "duplicate partition index should fail");
}

#[test]
fn validate_rejects_multiple_remaining() {
    let input = r#"
disk /dev/sda {
    label = gpt

    ext4 part_a {
        index = 1
        size = remaining
        mount = /a
    }

    ext4 part_b {
        index = 2
        size = remaining
        mount = /b
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    let result = validate(&ast);
    assert!(result.is_err(), "multiple remaining should fail");
}

#[test]
fn validate_rejects_subvol_outside_btrfs() {
    let input = r#"
disk /dev/sda {
    label = gpt

    ext4 root {
        index = 1
        size = 50G

        subvol @test {
            mount = /test
        }
    }
}
"#;
    // This should fail at parse time because subvol is only valid in btrfs,
    // but our grammar allows it in fs_body — so validation catches it.
    let ast = parse_storage(input).expect("should parse");
    let result = validate(&ast);
    assert!(result.is_err(), "subvol in ext4 should fail validation");
}

#[test]
fn validate_selinux_context_mutual_exclusivity() {
    let input = r#"
disk /dev/sda {
    label = gpt

    ext4 data {
        index = 1
        size = 100G

        mount {
            target = /srv/data
            context = system_u:object_r:data_t:s0
            defcontext = system_u:object_r:data_t:s0
        }
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    let result = validate(&ast);
    assert!(
        result.is_err(),
        "context + defcontext together should fail"
    );
}

#[test]
fn validate_luks_max_one_fs_without_lvm() {
    let input = r#"
disk /dev/sda {
    label = gpt

    luks2 system {
        index = 1
        size = remaining

        ext4 root { size = 50G; mount = / }
        ext4 home { size = 100G; mount = /home }
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    let result = validate(&ast);
    assert!(
        result.is_err(),
        "luks without lvm should allow at most 1 fs"
    );
}

#[test]
fn validate_mdraid_no_shared_disks() {
    let input = r#"
mdraid md0 {
    level = 1
    disks = [/dev/sda1, /dev/sdb1]

    ext4 boot {
        mount = /boot
    }
}

mdraid md1 {
    level = 1
    disks = [/dev/sda1, /dev/sdc1]

    ext4 data {
        mount = /data
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    let result = validate(&ast);
    assert!(
        result.is_err(),
        "/dev/sda1 in two arrays should fail"
    );
}

// ─── Raw Block ───────────────────────────────────────────────

#[test]
fn parse_raw_block() {
    let input = r#"
disk /dev/sda {
    label = gpt

    raw bios_boot {
        index = 1
        size = 1M
        type = ef02
    }

    ext4 root {
        index = 2
        size = remaining
        mount = /
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    if let StorageDecl::Disk(d) = &ast.declarations[0] {
        assert_eq!(d.children.len(), 2);
        assert!(matches!(&d.children[0], PartitionChild::Raw(_)));
    }
}

// ─── Swap Block ──────────────────────────────────────────────

#[test]
fn parse_swap_with_properties() {
    let input = r#"
disk /dev/sda {
    label = gpt

    swap swap0 {
        index = 1
        size = 32G
        priority = 100
        discard = true
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    if let StorageDecl::Disk(d) = &ast.declarations[0] {
        if let PartitionChild::Swap(s) = &d.children[0] {
            assert_eq!(s.name, "swap0");
            assert_eq!(s.properties.len(), 4);
        }
    }
}

// ─── Explicit Positioning ────────────────────────────────────

#[test]
fn parse_explicit_positioning() {
    let input = r#"
disk /dev/sdb {
    label = gpt

    xfs fast_tier {
        index = 1
        start = 1M
        end = 500G
        type = 8300
        mount = /srv/fast
    }

    xfs slow_tier {
        index = 2
        start = 500G
        end = -1
        type = 8300
        mount = /srv/bulk
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    assert_eq!(ast.declarations.len(), 1);
}

// ─── Comments ────────────────────────────────────────────────

#[test]
fn parse_with_comments() {
    let input = r#"
# This is a system disk
disk /dev/sda {
    label = gpt  # GPT partition table

    # EFI partition
    fat32 efi {
        index = 1
        size = 1G
        mount = /boot/efi
    }
}
"#;
    let ast = parse_storage(input).expect("should parse with comments");
    assert_eq!(ast.declarations.len(), 1);
}

// ─── NTFS Block ──────────────────────────────────────────────

#[test]
fn parse_ntfs() {
    let input = r#"
disk /dev/sda {
    label = gpt

    ntfs shared {
        index = 1
        size = 100G
        label = "SHARED"
        mount = /mnt/shared [nodev, nosuid, noexec]
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    if let StorageDecl::Disk(d) = &ast.declarations[0] {
        if let PartitionChild::Filesystem(f) = &d.children[0] {
            assert_eq!(f.fs_type, FsType::Ntfs);
        }
    }
}

// ─── Full RAID+LVM+Encryption Stack from Spec ───────────────

#[test]
fn parse_raid_lvm_encryption_stack() {
    let input = r#"
mdraid md0 {
    level = 1
    disks = [/dev/sda1, /dev/sdb1]
    metadata = 1.0

    ext4 boot {
        mount = /boot [nodev, nosuid, noexec]
    }
}

mdraid md1 {
    level = 10
    disks = [/dev/sda2, /dev/sdb2, /dev/sdc1, /dev/sdd1]
    chunk = 512K

    luks2 encrypted_array {
        cipher = aes-xts-plain64
        key_size = 512
        tpm2 = true

        lvm vg_data {
            thin pool0 {
                size = 90%

                xfs databases {
                    size = 500G
                    su = 256K
                    sw = 4
                    mount = /var/lib/postgres [nodev, nosuid]
                }

                xfs objects {
                    size = 2T
                    mount = /srv/objects [nodev, nosuid, noexec]
                }
            }

            ext4 logs {
                size = remaining
                mount = /var/log [nodev, nosuid, noexec]
            }
        }
    }
}
"#;
    let ast = parse_storage(input).expect("should parse complex stack");
    assert_eq!(ast.declarations.len(), 2);
    let warnings = validate(&ast).expect("should validate");
    let _ = warnings;
}

// ─── Error Reporting Quality ─────────────────────────────────

#[test]
fn error_reports_include_location() {
    let input = "disk /dev/sda { label = gpt }\ndisk /dev/sda { }";
    let ast = parse_storage(input).expect("should parse");
    let result = validate(&ast);
    match result {
        Err(crate::errors::IroncladError::ValidationError { errors }) => {
            // Should have error about missing label on second disk
            assert!(!errors.is_empty());
            let first = &errors[0];
            assert!(first.span.is_some());
        }
        _ => {
            // Second disk missing label — should be an error
            panic!("expected validation error");
        }
    }
}

// ─── Whole-Disk Encryption ───────────────────────────────────

#[test]
fn parse_whole_disk_encryption() {
    let input = r#"
disk /dev/sdd {
    label = none

    luks2 secure_scratch {
        xfs encrypted_data {
            mount = /mnt/secure
        }
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    let warnings = validate(&ast).expect("should validate");
    let _ = warnings;
}

// ─── luks1 ───────────────────────────────────────────────────

#[test]
fn parse_luks1() {
    let input = r#"
disk /dev/sda {
    label = gpt

    luks1 legacy_boot {
        index = 2
        size = 1G
        cipher = aes-xts-plain64
        key_size = 256

        ext4 boot_enc {
            mount = /boot
        }
    }
}
"#;
    let ast = parse_storage(input).expect("should parse");
    if let StorageDecl::Disk(d) = &ast.declarations[0] {
        if let PartitionChild::Luks(l) = &d.children[0] {
            assert_eq!(l.version, LuksVersion::Luks1);
        }
    }
}
