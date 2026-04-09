# Ironclad Storage Syntax Specification

**Status:** Draft — syntax development, Phase 1  
**Scope:** Disk, partitioning, encryption, integrity, volume management (LVM, ZFS, Stratis), RAID (mdraid), deduplication/compression (VDO), multipath, network storage (iSCSI, NFS), virtual filesystems (tmpfs), filesystems, mounts, and storage-level SELinux labeling

---

## Design Principles

Ironclad's storage syntax is a thin, structured wrapper over Linux storage tooling. Every block in the syntax maps to a real tool invocation — `parted`, `cryptsetup`, `pvcreate`/`lvcreate`, `mkfs.*`, `mount` — but the compiler handles ordering, validation, and interdependency resolution rather than the operator.

The key ergonomic properties:

1. **Nesting mirrors the real storage stack.** A filesystem inside an LVM volume inside a LUKS container inside a partition is written as exactly that nesting. The code reads like the actual dependency chain.
2. **Implicit ordering from structure.** The compiler topologically sorts operations from the declared hierarchy. The operator never specifies execution order.
3. **Named references.** Every named block becomes a referenceable identifier throughout the Ironclad source tree. A LUKS container named `system` can be referenced by key management declarations, clevis bindings, or firewall rules elsewhere.
4. **Validation from structure.** The compiler rejects impossible configurations at compile time — overlapping partitions, filesystems without backing devices, mount points referencing undeclared filesystems, LVM logical volumes exceeding volume group capacity, thin pool overcommit beyond configurable thresholds.
5. **Defaults that disappear the obvious.** When a default exists that a competent administrator would choose in nearly all cases, the language assumes it. Explicit declaration overrides any default. Every default is documented.
6. **Security labeling is a storage concern.** SELinux contexts on mount points are not decorative metadata — they define the trust boundary of every filesystem object created beneath that mount. The storage syntax carries enough label information for the compiler to emit correctly labeled mounts and validate those labels against the system's declared SELinux policy.

---

## Top-Level Blocks

Storage declarations begin with a top-level block that represents a physical or virtual block device.

### `disk`

Declares a physical block device and its partition table.

```
disk /dev/sda {
    label = gpt
}
```

**Required properties:**

| Property | Type | Description |
| --- | --- | --- |
| `label` | `gpt` \| `msdos` \| `none` | Partition table type. `none` indicates a whole-disk device with no partition table. |

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `sector_size` | integer | auto-detected | Override logical sector size (bytes). Relevant for 4Kn drives. |

**Children:** Partition blocks (filesystem type keywords, `luks2`, or `raw`) when `label` is `gpt` or `msdos`. A single filesystem block when `label` is `none`.

**Compiler behavior:** Emits `parted mklabel <label>` or skips partitioning entirely when `label = none`.

---

### `mdraid`

Declares a Linux software RAID array. Treated as a virtual block device — its children follow the same rules as `disk` children.

```
mdraid md0 {
    level = 10
    disks = [/dev/sdd, /dev/sde, /dev/sdf, /dev/sdg]
}
```

**Required properties:**

| Property | Type | Description |
| --- | --- | --- |
| `level` | `0` \| `1` \| `5` \| `6` \| `10` | RAID level. |
| `disks` | array of device paths | Member devices. |

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `spare` | array of device paths | none | Hot spare devices. |
| `chunk` | size string | kernel default | Chunk size for striped levels (`64K`, `512K`, etc.). |
| `bitmap` | `internal` \| `none` \| path | `internal` | Write-intent bitmap location. |
| `metadata` | `1.0` \| `1.1` \| `1.2` | `1.2` | Metadata version. `1.0` stores metadata at end of device (required for boot arrays on some bootloaders). |
| `layout` | string | kernel default | RAID layout. Level-specific (e.g., `f2` for RAID10 far layout). |
| `name` | string | array name | Human-readable name written to metadata. |

**Children:** Same as `disk` — filesystem blocks, `luks2`, `lvm`, or `raw`.

**Compiler behavior:** Emits `mdadm --create /dev/md/<n>` with the specified parameters. Member devices must either be raw partitions declared elsewhere in the source tree (compiler validates their existence) or assumed-present paths for pre-existing hardware.

---

### `zpool`

Declares a ZFS storage pool. ZFS is a combined volume manager and filesystem — zpools contain vdevs (virtual devices) that provide redundancy, and datasets or zvols that consume pool capacity. Unlike the LVM/mdraid/filesystem separation elsewhere in the storage syntax, ZFS collapses these layers into a single hierarchy.

```
zpool tank {
    vdev data-raidz1 {
        type = raidz1
        members = [/dev/sda, /dev/sdb, /dev/sdc]
    }
    
    vdev log-mirror {
        type = mirror
        members = [/dev/nvme0n1p1, /dev/nvme1n1p1]
    }
    
    vdev cache0 {
        type = cache
        members = [/dev/nvme2n1]
    }
    
    dataset root {
        mountpoint = /tank
        compression = zstd
        atime = false
    }
    
    dataset containers {
        mountpoint = /var/lib/containers
        compression = zstd
        quota = 500G
        reservation = 100G
    }
    
    zvol swap {
        size = 16G
    }
}
```

**Optional pool-level properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `ashift` | integer | auto-detected | Sector size exponent (`zpool create -o ashift=`). `12` for 4K drives, `9` for 512-byte. Auto-detection is correct on modern hardware; override only when you know the drive lies about sector size. |
| `autoexpand` | `true` \| `false` | `false` | Automatically expand pool when underlying devices grow. |
| `autotrim` | `true` \| `false` | `false` | Automatic TRIM/discard on SSDs. |
| `multihost` | `true` \| `false` | `false` | Multi-host protection (`zpool create -o multihost=on`). |
| `altroot` | path | none | Alternate root directory for pool import. |

**Children:** `vdev`, `dataset`, and `zvol` blocks.

**Compiler behavior:** Emits `zpool create <name>` with assembled vdev specifications. The compiler topologically sorts vdev types: data vdevs first, then log, then cache, then spare — matching ZFS's argument ordering. Requires the `zfs` package in the image package list.

---

#### `vdev`

A virtual device within a zpool. The `type` property determines the redundancy strategy.

```
vdev main {
    type = raidz2
    members = [/dev/sda, /dev/sdb, /dev/sdc, /dev/sdd, /dev/sde, /dev/sdf]
}
```

**Required properties:**

| Property | Type | Description |
| --- | --- | --- |
| `type` | `mirror` \| `raidz1` \| `raidz2` \| `raidz3` \| `stripe` \| `spare` \| `log` \| `cache` | Vdev topology type. `stripe` is the default ZFS behavior (no keyword emitted). `log` and `cache` are special-purpose vdevs — `log` provides a ZFS Intent Log (ZIL) for synchronous writes; `cache` provides an L2ARC read cache. |
| `members` | array of device paths | Member devices. |

**Compiler behavior:** For `mirror`, `raidz1`, `raidz2`, `raidz3`: emits the keyword followed by member devices in the `zpool create` argument list. For `stripe`: emits member devices with no keyword (ZFS default). For `log` and `cache`: emits the keyword as a separator in the vdev list. For `spare`: emits the `spare` keyword. A `log` vdev with multiple members and `type = mirror` on the vdev name implies `log mirror <members>`.

**Validation:**
- `raidz1` requires a minimum of 3 members.
- `raidz2` requires a minimum of 4 members.
- `raidz3` requires a minimum of 5 members.
- `mirror` requires a minimum of 2 members.
- `cache` and `spare` vdevs should not use the same devices as data or log vdevs.
- Member devices must not appear in more than one vdev within the same pool.

---

#### `dataset`

A ZFS dataset (filesystem) within a zpool. Datasets are the primary unit of data organization in ZFS and inherit properties from their parent pool or dataset.

```
dataset home {
    mountpoint = /home
    compression = zstd
    quota = 1T
    reservation = 200G
    atime = false
    exec = false
    setuid = false
    devices = false
}
```

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `mountpoint` | path \| `none` | `/<pool>/<dataset>` | Mount point (`zfs set mountpoint=`). `none` creates the dataset without mounting it. |
| `compression` | `off` \| `lz4` \| `zstd` \| `zstd:<level>` \| `gzip` \| `gzip:<level>` \| `lzjb` \| `zle` | `off` | Compression algorithm (`zfs set compression=`). |
| `quota` | size string | none | Maximum space this dataset can consume (`zfs set quota=`). |
| `refquota` | size string | none | Maximum space for this dataset excluding snapshots and children (`zfs set refquota=`). |
| `reservation` | size string | none | Guaranteed space allocation (`zfs set reservation=`). |
| `refreservation` | size string | none | Guaranteed space excluding snapshots and children (`zfs set refreservation=`). |
| `recordsize` | size string | `128K` | Maximum record size (`zfs set recordsize=`). Tune for workload: `16K` for databases, `1M` for sequential large files. |
| `atime` | `true` \| `false` | `true` | Update access time on reads (`zfs set atime=`). |
| `relatime` | `true` \| `false` | `false` | Only update atime when mtime/ctime changes (`zfs set relatime=`). |
| `exec` | `true` \| `false` | `true` | Allow execution of binaries (`zfs set exec=`). |
| `setuid` | `true` \| `false` | `true` | Allow setuid binaries (`zfs set setuid=`). |
| `devices` | `true` \| `false` | `true` | Allow device nodes (`zfs set devices=`). |
| `dedup` | `off` \| `on` \| `verify` \| `sha256` \| `sha512` \| `skein` | `off` | Deduplication (`zfs set dedup=`). Extreme RAM cost — approximately 5GB per TB of data. |
| `copies` | `1` \| `2` \| `3` | `1` | Number of data copies (`zfs set copies=`). |
| `snapdir` | `visible` \| `hidden` | `hidden` | Visibility of `.zfs/snapshot` directory. |
| `xattr` | `on` \| `off` \| `sa` | `on` | Extended attribute handling. `sa` stores xattrs in system attributes (preferred on Linux for SELinux). |
| `dnodesize` | `legacy` \| `auto` \| `1k` \| `2k` \| `4k` \| `8k` \| `16k` | `legacy` | Dnode size for metadata-heavy workloads. |
| `encryption` | `off` \| `aes-256-gcm` \| `aes-256-ccm` | `off` | Native ZFS encryption. Encrypts data at the dataset level — different datasets can have different keys. |
| `keyformat` | `passphrase` \| `hex` \| `raw` | `passphrase` | Encryption key format (when `encryption` is not `off`). |
| `keylocation` | `prompt` \| URL \| path | `prompt` | Where to load the encryption key from. |
| `context` | SELinux context expression | none | SELinux mount context. Applied via `-o context=` when mounting. |

**Children:** Nested `dataset` blocks for hierarchical dataset organization.

**Compiler behavior:** Emits `zfs create <pool>/<dataset>` with `-o` flags for each declared property. Nested datasets emit as `<pool>/<parent>/<child>`. When `encryption` is not `off`, emits `zfs create -o encryption=<alg> -o keyformat=<fmt> -o keylocation=<loc>`.

**SELinux note:** ZFS with `xattr = sa` supports extended attributes and therefore supports per-file SELinux labeling. ZFS with `xattr = off` does not and requires `context=` mount semantics, same as `fat32`. The compiler enforces this distinction.

---

#### `zvol`

A ZFS volume — a block device backed by a zpool. Used for swap, iSCSI targets, or scenarios requiring a raw block device with ZFS's data management underneath.

```
zvol swap {
    size = 16G
    compression = lz4
    volblocksize = 4K
}
```

**Required properties:**

| Property | Type | Description |
| --- | --- | --- |
| `size` | size string | Volume size (`zfs create -V`). |

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `volblocksize` | size string | `8K` | Block size for the volume (`zfs create -b`). |
| `compression` | same as `dataset` | `off` | Compression algorithm. |
| `dedup` | same as `dataset` | `off` | Deduplication. |
| `reservation` | size string | equals `size` | Space reservation. ZFS reserves the full volume size by default (thick provisioning). |
| `sparse` | `true` \| `false` | `false` | Thin provisioning — do not reserve the full volume size. (`zfs create -s -V`). |

**Children:** `swap` blocks, or treated as a raw block device for use by `luks2`, filesystem blocks, etc.

**Compiler behavior:** Emits `zfs create -V <size> <pool>/<name>`. The resulting block device appears at `/dev/zvol/<pool>/<name>`.

---

### `stratis`

Declares a Stratis managed storage pool. Stratis is Red Hat's strategic local storage management solution, providing a ZFS-like experience within the RHEL ecosystem — managed pools with thin-provisioned filesystems, snapshots, and optional encryption. Stratis pools always produce XFS filesystems.

```
stratis appdata {
    disks = [/dev/sda, /dev/sdb]
    encrypted = true
    
    filesystem web {
        mountpoint = /srv/www
        size_limit = 200G
    }
    
    filesystem database {
        mountpoint = /var/lib/postgres
        size_limit = 500G
    }
}
```

**Required properties:**

| Property | Type | Description |
| --- | --- | --- |
| `disks` | array of device paths | Block devices forming the pool's data tier. |

**Optional pool-level properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `encrypted` | `true` \| `false` | `false` | Encrypt the pool at the block layer (`stratis pool create --key-desc`). Uses kernel keyring for key management. |
| `key_desc` | string | pool name | Kernel keyring key description for encryption. |
| `cache` | array of device paths | none | Block devices for the pool's cache tier (`stratis pool init-cache`). Typically SSDs. |
| `cache_mode` | `writeback` \| `writethrough` | `writethrough` | Cache write policy. `writeback` improves performance but risks data loss on cache failure. |
| `overprovision` | `true` \| `false` | `true` | Allow thin-provisioned filesystems to overcommit pool capacity. |

**Children:** `filesystem` blocks.

**Compiler behavior:** Emits `stratis pool create <name> <disks>`. If `encrypted = true`, emits key setup via `stratis key set` before pool creation. If `cache` is specified, emits `stratis pool init-cache <name> <cache_disks>` after pool creation. Requires the `stratisd` and `stratis-cli` packages in the image.

---

#### `filesystem` (Stratis)

A thin-provisioned XFS filesystem within a Stratis pool. Stratis filesystems are always XFS — the filesystem type is not configurable.

```
filesystem containers {
    mountpoint = /var/lib/containers
    size_limit = 1T
}
```

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `mountpoint` | path | none | Mount point. |
| `size_limit` | size string | none | Filesystem size limit. Stratis filesystems are thin-provisioned and grow on demand; `size_limit` caps the maximum. Without a limit, the filesystem can consume the entire pool. |
| `options` | array of strings | `[defaults]` | Additional mount options. |
| `context` | SELinux context expression | none | SELinux mount context. |

**Compiler behavior:** Emits `stratis filesystem create <pool> <name>`. If `size_limit` is set, emits `stratis filesystem create --size-limit <limit>`. Mount entries use the Stratis device path `/dev/stratis/<pool>/<filesystem>`. Fstab entries use `x-systemd.requires=stratisd.service` to ensure the Stratis daemon is running before mount.

**SELinux note:** Stratis filesystems are XFS and support extended attributes. Per-file SELinux labeling works normally. Mount-level context is optional.

---

### `multipath`

Declares a device-mapper multipath device for SAN-attached storage with redundant paths. Multipath is a top-level block that presents multiple physical paths to the same LUN as a single virtual device.

```
multipath mpath0 {
    wwid = "3600508b4000c4a37000009000012a000"
    policy = round-robin
    
    path /dev/sda {
        priority_group = 1
    }
    
    path /dev/sdb {
        priority_group = 1
    }
    
    path /dev/sdc {
        priority_group = 2
    }
    
    path /dev/sdd {
        priority_group = 2
    }
    
    luks2 encrypted_san {
        lvm vg_san {
            xfs data { size = remaining; mount = /srv/san }
        }
    }
}
```

**Required properties:**

| Property | Type | Description |
| --- | --- | --- |
| `wwid` | string | World Wide Identifier of the LUN. Used to correlate paths. |

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `policy` | `round-robin` \| `queue-length` \| `service-time` | `service-time` | Path selection policy (`multipathd` path selector). |
| `failback` | `immediate` \| `manual` \| integer (seconds) | `immediate` | Failback behavior when a failed path recovers. |
| `no_path_retry` | `fail` \| `queue` \| integer | `fail` | Behavior when all paths fail. `queue` holds I/O until a path returns; integer specifies retry count before failing. |
| `rr_min_io` | integer | `1000` | Minimum I/O requests per path before switching in round-robin. |
| `checker` | `tur` \| `readsector0` \| `directio` | `tur` | Path health checker. `tur` (Test Unit Ready) is preferred for most hardware. |
| `features` | array of strings | none | DM multipath features (e.g., `[queue_if_no_path, retain_attached_hw_handler]`). |

**Children:** `path` blocks declaring individual paths, followed by the same children as `disk` — filesystem blocks, `luks2`, `lvm`, or `raw`.

**Compiler behavior:** Generates `/etc/multipath.conf` entries for the device and emits `multipath -r` to reconfigure. The multipath device appears at `/dev/mapper/<name>`. Requires `device-mapper-multipath` in the image package list.

---

#### `path`

An individual physical path within a `multipath` block.

```
path /dev/sda {
    priority_group = 1
}
```

**Properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `priority_group` | integer | `1` | Priority group number. Lower groups are preferred. All paths in the same group are active simultaneously under the parent's path selection policy; higher-numbered groups are standby. |

---

## Partition-Level Blocks

Direct children of a `disk` block represent partitions. The block keyword is the filesystem type that will be created on the partition, or a structural keyword (`luks2`, `raw`).

### Common Partition Properties

Every direct child of a `disk` block accepts these properties:

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `index` | integer | declaration order | Partition number in the table. Explicit when order matters for bootloader compatibility; otherwise inferred from source order. |
| `size` | size string | required unless `start`/`end` given | Partition size. Accepts `1G`, `500M`, `50%`, `remaining`. |
| `start` | size string | auto-calculated | Explicit start offset from beginning of disk. Not recommended — prefer `size` and let the compiler calculate. |
| `end` | size string | auto-calculated | Explicit end offset. `-1` means end of disk. |
| `align` | size string | optimal for device | Alignment boundary. Override only when you know why. |
| `type` | string | inferred from context | Partition type GUID (GPT) or partition ID (MBR). Common values: `ef00` (EFI System), `ef02` (BIOS boot), `8300` (Linux filesystem), `8200` (Linux swap), `8309` (Linux LUKS), `8e00` (Linux LVM). When omitted, the compiler infers from the block type — `fat32` as a first partition infers `ef00`; `swap` infers `8200`; `luks2` infers `8309`. |

**Size strings:** A number followed by a unit. Valid units: `B`, `K`, `KB`, `M`, `MB`, `G`, `GB`, `T`, `TB`. `%` indicates percentage of the parent container's available space. `remaining` consumes all unallocated space in the parent. Only one `remaining` is permitted per parent scope.

---

### `raw`

A partition with no filesystem. Used for BIOS boot partitions, reserved regions, or partitions managed by external tooling.

```
raw bios_boot {
    index = 1
    size = 1M
    type = ef02
}
```

**Compiler behavior:** Emits `parted mkpart` with the specified boundaries. No `mkfs` or `mount` is generated.

---

## Filesystem Type Keywords

Filesystem type keywords serve as block identifiers that simultaneously declare the partition (when inside `disk`) or logical volume (when inside `lvm`) **and** the filesystem to create on it. The keyword determines which `mkfs` variant the compiler emits.

### `ext4`

```
ext4 boot {
    size = 1G
    mount = /boot [nodev, nosuid, noexec]
}
```

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `label` | string | block name | Filesystem label (`mkfs.ext4 -L`). |
| `block_size` | integer | 4096 | Block size in bytes (`mkfs.ext4 -b`). |
| `reserved_blocks` | percentage | `5%` | Reserved block percentage (`tune2fs -m`). |
| `features` | array of strings | mkfs defaults | Feature flags (`mkfs.ext4 -O`). e.g., `[metadata_csum, 64bit]`. |
| `inode_size` | integer | 256 | Inode size in bytes (`mkfs.ext4 -I`). |
| `inode_ratio` | integer | mkfs default | Bytes-per-inode ratio (`mkfs.ext4 -i`). Controls inode density. |
| `journal` | `true` \| `false` \| size string | `true` | External journal size or disable. |
| `stride` | integer | none | RAID stride in filesystem blocks (`mkfs.ext4 -E stride=`). |
| `stripe_width` | integer | none | RAID stripe width in filesystem blocks (`mkfs.ext4 -E stripe-width=`). |
| `mount` | mount expression | none | Mount target and options. See Mount Expressions. |

**Compiler behavior:** Emits `mkfs.ext4` with mapped flags. When `reserved_blocks` differs from default, emits a follow-up `tune2fs -m` call.

---

### `xfs`

```
xfs data {
    size = 500G
    mount = /srv/data
}
```

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `label` | string | block name | Filesystem label (`mkfs.xfs -L`). |
| `block_size` | integer | 4096 | Block size (`mkfs.xfs -b size=`). |
| `su` | size string | none | Stripe unit (`mkfs.xfs -d su=`). For hardware or software RAID alignment. |
| `sw` | integer | none | Stripe width — number of data disks (`mkfs.xfs -d sw=`). |
| `log_size` | size string | auto | Internal log size (`mkfs.xfs -l size=`). |
| `log_device` | device path | none | External log device (`mkfs.xfs -l logdev=`). |
| `reflink` | `true` \| `false` | `true` | Enable reflink support (`mkfs.xfs -m reflink=`). |
| `bigtime` | `true` \| `false` | `true` | Timestamps beyond 2038 (`mkfs.xfs -m bigtime=`). |
| `mount` | mount expression | none | Mount target and options. |

**Compiler behavior:** Emits `mkfs.xfs` with mapped flags.

---

### `btrfs`

Btrfs blocks can contain `subvol` children declaring named subvolumes.

```
btrfs root {
    size = remaining
    compress = zstd:1
    
    subvol @ {
        mount = /
    }
    
    subvol @home {
        mount = /home [nodev, nosuid, compress=zstd:3]
        quota = 50G
    }
}
```

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `label` | string | block name | Filesystem label (`mkfs.btrfs -L`). |
| `features` | array of strings | mkfs defaults | Feature flags (`mkfs.btrfs -O`). e.g., `[quota, free-space-tree]`. |
| `compress` | string | none | Default compression algorithm and level. Applied as mount option. Valid: `zstd`, `zstd:<level>`, `lzo`, `zlib`, `zlib:<level>`. |
| `node_size` | size string | 16K | Metadata node size (`mkfs.btrfs -n`). |
| `sector_size` | integer | auto | Sector size (`mkfs.btrfs -s`). |
| `data_profile` | `single` \| `dup` \| `raid0` \| `raid1` \| `raid1c3` \| `raid1c4` \| `raid10` \| `raid5` \| `raid6` | `single` | Data block group profile (`mkfs.btrfs -d`). |
| `metadata_profile` | same as `data_profile` | `dup` | Metadata block group profile (`mkfs.btrfs -m`). |
| `mount` | mount expression | none | Mount target for the filesystem root (rarely used directly — prefer `subvol` mounts). |

**Children:** `subvol` blocks.

**Compiler behavior:** Emits `mkfs.btrfs`, then `btrfs subvolume create` for each declared subvolume. Mounts the filesystem temporarily to a staging path to create subvolumes, then unmounts and remounts each subvolume at its declared mount point.

---

#### `subvol`

A Btrfs subvolume. Only valid inside a `btrfs` block. The name after the keyword is the actual subvolume name passed to `btrfs subvolume create`.

```
subvol @var_log {
    mount = /var/log [nodev, nosuid, noexec]
    quota = 10G
}
```

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `mount` | mount expression | none | Mount target and options. The `subvol=<n>` mount option is automatically appended by the compiler. |
| `quota` | size string | none | Btrfs qgroup limit for this subvolume. Requires `quota` in parent's `features`. |
| `compress` | string | inherited from parent | Override parent compression for this subvolume (applied as mount option). |

**Compiler behavior:** Emits `btrfs subvolume create <parent_mount>/<n>`. If `quota` is set, emits `btrfs qgroup limit <size> <parent_mount>/<n>`.

---

### `fat32`

Used primarily for EFI System Partitions.

```
fat32 efi {
    size = 1G
    type = ef00
    mount = /boot/efi [nodev, nosuid, noexec]
}
```

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `label` | string | block name (uppercased, truncated to 11 chars) | Volume label (`mkfs.fat -n`). |
| `fat_size` | `12` \| `16` \| `32` | `32` | FAT variant (`mkfs.fat -F`). |
| `cluster_size` | integer | auto | Cluster size in bytes (`mkfs.fat -s`). |
| `mount` | mount expression | none | Mount target and options. |

**Compiler behavior:** Emits `mkfs.fat -F 32` (or specified variant).

**SELinux note:** `fat32` does not support extended attributes. The compiler enforces that any `fat32` filesystem with a `mount` declaration must have an explicit `context` in its mount expression when the system's SELinux mode is `mls` or `strict` security floor is active. Without `context=`, all files under the mount inherit an unlabeled type, which MLS policy will deny access to. See [SELinux Context on Mount Expressions](#selinux-context-on-mount-expressions).

---

### `swap`

Swap is not a filesystem — it uses `mkswap` rather than any `mkfs` variant. It is its own keyword to reflect this.

```
swap swap0 {
    size = 32G
}
```

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `label` | string | block name | Swap label (`mkswap -L`). |
| `priority` | integer | none | Swap priority for `swapon -p` and fstab. Higher values = preferred. |
| `discard` | `true` \| `false` | `false` | Enable discard/TRIM on swap (`swapon -d`). |
| `page_size` | integer | system default | Override page size (`mkswap -p`). Rare. |

**Compiler behavior:** Emits `mkswap`. No `mount` — the compiler generates the fstab swap entry and `swapon` invocation automatically.

---

### `ntfs`

Included for dual-boot and data exchange scenarios.

```
ntfs shared {
    size = 100G
    label = "SHARED"
    mount = /mnt/shared [nodev, nosuid, noexec]
}
```

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `label` | string | block name | Volume label (`mkfs.ntfs -L`). |
| `compression` | `true` \| `false` | `false` | Enable NTFS compression. |
| `quick` | `true` \| `false` | `true` | Quick format — skip full surface scan (`mkfs.ntfs -Q`). |
| `mount` | mount expression | none | Mount target and options. |

**Compiler behavior:** Emits `mkfs.ntfs`. Requires `ntfs-3g` in the image package list.

**SELinux note:** `ntfs` does not support extended attributes. Same enforcement rules as `fat32` — explicit `context` required under MLS. See [SELinux Context on Mount Expressions](#selinux-context-on-mount-expressions).

---

### `tmpfs`

Declares a tmpfs mount — a RAM-backed filesystem. Not a block device, but a declared mount with properties that the compiler must track for fstab generation, SELinux labeling, and security floor enforcement. Nearly every hardened system explicitly declares `/tmp`, `/run`, `/dev/shm`, and `/var/tmp` as tmpfs.

```
tmpfs tmp {
    mount = /tmp [nodev, nosuid, noexec] context system_u:object_r:tmp_t:s0
    size = 2G
}

tmpfs devshm {
    mount = /dev/shm [nodev, nosuid, noexec]
    size = 50%
}

tmpfs run {
    mount = /run [nodev, nosuid] context system_u:object_r:var_run_t:s0
}
```

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `size` | size string \| percentage | `50%` | Maximum size. Percentage is relative to total system RAM. `50%` is the kernel default. |
| `nr_inodes` | integer \| `unlimited` | kernel default | Maximum number of inodes. |
| `nr_blocks` | integer | derived from `size` | Maximum number of blocks. Rarely specified directly — use `size` instead. |
| `mode` | octal string | `1777` for `/tmp`, `0755` for others | Root directory permissions. |
| `uid` | integer \| string | `0` | Owner UID or username. |
| `gid` | integer \| string | `0` | Owner GID or group name. |
| `huge` | `never` \| `always` \| `within_size` \| `advise` | `never` | Huge page allocation policy (`mount -o huge=`). |
| `mount` | mount expression | required | Mount target and options. |

**Compiler behavior:** Emits an fstab entry with `tmpfs` as the filesystem type and `tmpfs` as the device. Mount options include `size=`, `nr_inodes=`, `mode=`, and SELinux context options as specified. No `mkfs` is emitted — tmpfs requires no formatting.

**SELinux note:** `tmpfs` does not support xattr-based labeling. All labeling must be done via mount-level `context=`. The compiler enforces this under MLS: a `tmpfs` block without a `context` declaration when the security floor is `strict` or higher is an error.

**Security floor interaction:** Under `standard` and above, `/tmp` must have `nodev, nosuid, noexec`. Under `strict` and above, `/dev/shm` must have `nodev, nosuid, noexec`. These are enforced on `tmpfs` blocks whose `mount` target matches these paths.

---

## Encryption Blocks

### `luks2`

Declares a LUKS2 encrypted container. Can wrap filesystems directly, or contain an `lvm` block for volume management inside the encrypted layer.

```
luks2 system {
    index = 2
    size = remaining
    type = 8309
    cipher = aes-xts-plain64
    key_size = 512
    
    lvm vg0 {
        ext4 root { size = 50G; mount = / }
    }
}
```

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `cipher` | string | `aes-xts-plain64` | Encryption cipher (`cryptsetup luksFormat --cipher`). |
| `key_size` | integer | `512` | Key size in bits (`cryptsetup luksFormat --key-size`). |
| `hash` | string | `sha512` | PBKDF hash algorithm (`--hash`). |
| `pbkdf` | string | `argon2id` | Key derivation function (`--pbkdf`). |
| `iter_time` | integer | `5000` | PBKDF iteration time in milliseconds (`--iter-time`). |
| `sector_size` | integer | `4096` | Encryption sector size (`--sector-size`). |
| `integrity` | string | none | dm-integrity algorithm (`--integrity`). e.g., `hmac-sha256`. Enables authenticated encryption. Significant write performance cost. |
| `tpm2` | `true` \| `false` | `false` | Bind key to TPM 2.0 via Clevis (`clevis luks bind tpm2`). |
| `tang` | URL string | none | Bind key to a Tang server via Clevis (`clevis luks bind tang`). Can coexist with `tpm2` for Shamir Secret Sharing (SSS) policy. |
| `tang_thp` | string | none | Tang server thumbprint for offline provisioning without trust-on-first-use. |
| `header` | path string | none | Detached LUKS header location. Stores the LUKS header on a separate device or file, leaving the data partition with no visible encryption metadata. |
| `label` | string | block name | LUKS label (`--label`). |

**Children:** A single filesystem block (direct encryption of one filesystem), or an `lvm` block (encryption wrapping a volume group), or another structural block.

**Compiler behavior:** Emits `cryptsetup luksFormat` with mapped flags, followed by `cryptsetup open`. If `tpm2` or `tang` is set, emits the corresponding `clevis luks bind` commands. The opened device name is derived from the block name (`/dev/mapper/<n>`).

---

### `luks1`

Provided for compatibility with systems that require LUKS1 (e.g., GRUB2 boot partition encryption on older configurations). Properties mirror `luks2` except `integrity` and `sector_size` are not available.

```
luks1 legacy_boot {
    index = 2
    size = 1G
    cipher = aes-xts-plain64
    key_size = 256
}
```

**Compiler behavior:** Emits `cryptsetup luksFormat --type luks1`.

---

### `integrity`

Declares a dm-integrity block device providing sector-level data integrity verification. dm-integrity stores checksums alongside data, detecting silent corruption (bitrot) at the block layer before it propagates to filesystems.

dm-integrity can operate standalone (data integrity only) or paired with LUKS2 (authenticated encryption — confidentiality plus integrity). When declared inside a `luks2` block, the `integrity` property on the LUKS block itself is the preferred syntax (see `luks2` above). The standalone `integrity` block is for scenarios where integrity verification is desired without encryption.

```
integrity verified_data {
    algorithm = crc32c
    
    xfs data {
        size = remaining
        mount = /srv/verified [nodev, nosuid, noexec]
    }
}
```

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `algorithm` | `crc32c` \| `crc32` \| `sha1` \| `sha256` \| `hmac-sha256` \| `hmac-sha512` | `crc32c` | Integrity hash algorithm. `crc32c` is fastest with hardware acceleration on modern x86. `sha256` and above provide cryptographic integrity but at significant I/O cost. `hmac-*` variants require a key and provide authentication — use these when tampering detection (not just corruption detection) is required. |
| `tag_size` | integer | algorithm-dependent | Integrity tag size in bytes. Automatically sized for the chosen algorithm; override only for non-standard configurations. |
| `sector_size` | integer | `512` | Protected sector size. `4096` reduces metadata overhead on 4Kn drives. |
| `journal` | `true` \| `false` | `true` | Enable the dm-integrity write journal. The journal ensures atomicity of data+tag writes — without it, a crash can leave data and its tag inconsistent. Disabling improves write performance but risks false corruption reports after unclean shutdown. |
| `journal_size` | size string | auto | Journal size. Larger journals absorb longer write bursts before stalling. |
| `recalculate` | `true` \| `false` | `false` | Recalculate integrity tags on first activation. Required when adding integrity to a device with existing data. Runs in the background. |
| `mode` | `journal` \| `bitmap` \| `direct` | `journal` | Write mode. `journal` provides full atomicity. `bitmap` tracks dirty sectors for faster recovery but without full atomicity. `direct` writes data and tags directly with no crash protection — fast but unsafe. |

**Children:** Filesystem blocks, `lvm`, or `swap`.

**Compiler behavior:** Emits `integritysetup format <device> --integrity <algorithm>` followed by `integritysetup open <device> <n>`. The opened device appears at `/dev/mapper/<n>`. Requires `integritysetup` (part of `cryptsetup` package).

**Performance note:** dm-integrity adds measurable write latency due to tag computation and journaling. With `crc32c` and journaling, expect approximately 10-20% write throughput reduction. With `hmac-sha256` and journaling, expect 40-60% reduction. Read impact is smaller — tags are verified inline during reads. Profile your workload before deploying in performance-sensitive environments.

**Validation:**
- `hmac-*` algorithms require a key source. When used standalone (not inside LUKS2), the compiler requires a `key` property pointing to a keyfile or kernel keyring descriptor.
- When nested inside `luks2`, the `integrity` property on the LUKS block is used instead of a separate `integrity` block. Declaring both is an error.

---

## Volume Management Blocks

### `lvm`

Declares an LVM volume group. Contains logical volume children (filesystem or swap blocks) and optionally `thin` pool blocks.

```
lvm vg0 {
    ext4 root { size = 50G; mount = / }
    swap swap0 { size = 16G }
    
    thin pool0 {
        size = 200G
        
        xfs data { size = 300G; mount = /srv }
    }
    
    xfs scratch { size = remaining; mount = /tmp }
}
```

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `pe_size` | size string | `4M` | Physical extent size (`vgcreate -s`). |
| `max_lv` | integer | unlimited | Maximum logical volumes (`vgcreate -l`). |
| `clustered` | `true` \| `false` | `false` | Clustered volume group flag. |
| `tags` | array of strings | none | LVM tags applied to the VG. |

**Children:** Filesystem blocks (thick logical volumes), `swap` blocks, and `thin` pool blocks. Direct children of `lvm` are standard (thick) logical volumes. Their `size` is physically allocated.

**Compiler behavior:** Emits `pvcreate` on the parent block device, `vgcreate <n>`, then `lvcreate` for each child volume in declaration order. The volume is named `/dev/<vg_name>/<lv_name>` where `lv_name` is the child block's name.

---

#### `thin`

A thin provisioning pool inside an LVM volume group. Only valid inside an `lvm` block. Children are thin logical volumes — their `size` is a virtual size that can exceed the pool's physical allocation (overprovisioning).

```
thin pool0 {
    size = 200G
    chunk_size = 64K
    
    xfs data { size = 300G; mount = /srv }
    ext4 containers { size = 150G; mount = /var/lib/containers }
}
```

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `size` | size string | required | Physical size of the thin pool. |
| `chunk_size` | size string | auto | Thin pool chunk size (`lvcreate --chunksize`). Smaller = finer allocation granularity, larger = better sequential performance. |
| `metadata_size` | size string | auto | Metadata LV size. Rarely needs manual specification. |
| `zero` | `true` \| `false` | `true` | Zero new allocations (`--zero y/n`). |
| `discard` | `true` \| `false` | `true` | Support discard/TRIM passthrough. |
| `overcommit_warn` | percentage | `80%` | Compiler emits a warning when total virtual allocation exceeds this percentage of pool physical size. |
| `overcommit_deny` | percentage | none | Compiler refuses to compile when total virtual allocation exceeds this percentage. |

**Children:** Filesystem blocks. These become thin logical volumes in the pool. Their `size` is virtual.

**Compiler behavior:** Emits `lvcreate --thin --size <pool_size> <vg>/<pool_name>`, then `lvcreate --thin --virtualsize <lv_size> <vg>/<pool_name> --name <lv_name>` for each child.

---

#### `vdo`

An LVM VDO (Virtual Data Optimizer) logical volume providing inline deduplication and compression at the block layer. Only valid inside an `lvm` block. On RHEL 9+, VDO is integrated directly into LVM as `lvm_vdo` — the standalone VDO module is deprecated.

```
lvm vg0 {
    vdo dedup_pool {
        size = 500G
        virtual_size = 2T
        deduplication = true
        compression = true
        
        xfs data {
            mount = /srv/data [nodev, nosuid, noexec]
        }
    }
}
```

**Required properties:**

| Property | Type | Description |
| --- | --- | --- |
| `size` | size string | Physical size of the VDO pool (actual disk space consumed). |
| `virtual_size` | size string | Virtual size presented to the filesystem above. Can exceed physical size due to deduplication and compression. |

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `deduplication` | `true` \| `false` | `true` | Enable block-level deduplication. |
| `compression` | `true` \| `false` | `true` | Enable block-level compression. |
| `write_policy` | `auto` \| `sync` \| `async` | `auto` | Write policy. `auto` selects based on underlying storage. `sync` for data integrity on non-battery-backed storage; `async` for performance when write-back caching is safe. |
| `slab_size` | size string | `2G` | VDO slab size. Each slab consumes approximately 824 bytes of VDO metadata. Larger slabs reduce metadata overhead for large pools. Valid: `128M` through `32G`, powers of 2. |
| `block_map_cache_size` | size string | `128M` | Size of the block map cache in RAM. Larger values improve random read performance. |
| `block_map_period` | integer | `16380` | Block map era length. Controls how often the block map is flushed to disk. |
| `index_memory` | size string | `256M` | UDS index memory. Controls deduplication index size. `256M` handles approximately 1TB of unique data. Scale proportionally. |
| `sparse_index` | `true` \| `false` | `false` | Use a sparse UDS index. Reduces RAM usage at the cost of deduplication effectiveness. |
| `emulate_512` | `true` \| `false` | `false` | Emulate 512-byte sectors for legacy applications. |
| `overcommit_warn` | percentage | `80%` | Warning when virtual allocation reaches this percentage of estimated deduplication-adjusted capacity. |

**Children:** A single filesystem block or `swap` block.

**Compiler behavior:** Emits `lvcreate --type vdo --name <n> --size <size> --virtualsize <virtual_size> <vg>`. Followed by VDO property configuration via `lvchange`. Requires LVM with VDO support (`lvm2` package on RHEL 9+).

**Validation:**
- `virtual_size` must be greater than or equal to `size`.
- `slab_size` must be a power of 2 between `128M` and `32G`.
- Physical size must be at least `5G` (VDO minimum for metadata and slabs).

---

---

## Network Storage

Network-attached storage is not a local block device, but it is declared storage with mount targets, credentials, failure behavior, and SELinux labeling that the system definition must capture. The compiler tracks network mounts for fstab generation, dependency ordering, and security floor enforcement.

### `iscsi`

Declares an iSCSI target connection. The resulting block device is treated as a local device and can contain the same children as `disk` — filesystems, LUKS, LVM, or raw.

```
iscsi san_lun0 {
    target = "iqn.2024.com.example:storage.lun0"
    portal = "10.0.1.50:3260"
    auth = chap
    username = "initiator01"
    
    luks2 encrypted_san {
        lvm vg_san {
            xfs data { size = remaining; mount = /srv/san [nodev, nosuid, noexec] }
        }
    }
}
```

**Required properties:**

| Property | Type | Description |
| --- | --- | --- |
| `target` | string | iSCSI Qualified Name (IQN) of the target. |
| `portal` | string | Target portal address (`host:port`). Port defaults to 3260 if omitted. |

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `auth` | `none` \| `chap` \| `chap_mutual` | `none` | Authentication method. `chap` uses one-way CHAP; `chap_mutual` uses mutual CHAP where the target also authenticates to the initiator. |
| `username` | string | none | CHAP username (required when `auth` is `chap` or `chap_mutual`). |
| `initiator_name` | string | system default | Override the iSCSI initiator name (IQN) for this connection. |
| `lun` | integer | `0` | Logical Unit Number to connect to on the target. |
| `auto_login` | `true` \| `false` | `true` | Automatically log in to the target at boot. |
| `replacement_timeout` | integer | `120` | Seconds to wait for session recovery before failing I/O. |
| `startup` | `automatic` \| `onboot` \| `manual` | `automatic` | When to establish the iSCSI session. `onboot` connects before network filesystems; `automatic` connects after. |
| `multipath` | `true` \| `false` | `false` | When `true`, the iSCSI device feeds into a `multipath` block rather than being used directly. |
| `portals` | array of strings | none | Additional portal addresses for multipath or failover. |

**Children:** Same as `disk` — filesystem blocks, `luks2`, `lvm`, `raw`.

**Compiler behavior:** Configures `/etc/iscsi/iscsid.conf` for the session, emits `iscsiadm -m discovery -t sendtargets -p <portal>` and `iscsiadm -m node -T <target> -p <portal> --login` during provisioning. Generates fstab entries with `_netdev` and `x-systemd.requires=iscsi.service`. Requires `iscsi-initiator-utils` in the image package list.

**Validation:**
- `username` is required when `auth` is not `none`.
- Credentials are references to the system's secret management declarations — the compiler does not accept plaintext passwords in storage syntax.

---

### `nfs`

Declares an NFS mount. NFS mounts are not block devices and cannot contain children — they declare a remote filesystem mount with connection parameters.

```
nfs home_nfs {
    server = "nfs.internal.example.com"
    export = "/exports/home"
    version = 4.2
    
    mount {
        target = /home
        options = [nodev, nosuid, sec=krb5p, hard, intr]
        requires = [network-online.target, gssproxy.service]
        context = system_u:object_r:home_root_t:s0
    }
}
```

**Required properties:**

| Property | Type | Description |
| --- | --- | --- |
| `server` | string | NFS server hostname or IP address. |
| `export` | path | Server-side export path. |

**Optional properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `version` | `3` \| `4` \| `4.1` \| `4.2` | `4.2` | NFS protocol version (`mount -o vers=`). |
| `sec` | `sys` \| `krb5` \| `krb5i` \| `krb5p` | `sys` | Security flavor. `sys` uses AUTH_SYS (UID/GID). `krb5` uses Kerberos authentication. `krb5i` adds integrity checking. `krb5p` adds encryption. |
| `proto` | `tcp` \| `udp` \| `rdma` | `tcp` | Transport protocol. |
| `rsize` | size string | negotiated | Read buffer size. |
| `wsize` | size string | negotiated | Write buffer size. |
| `timeo` | integer | `600` | Timeout in tenths of a second for NFS requests. |
| `retrans` | integer | `2` | Number of retransmissions before failing. |
| `hard` | `true` \| `false` | `true` | Hard mount — retry indefinitely on server failure. `false` produces a soft mount that returns errors after `retrans` attempts. |
| `mount` | mount expression or mount block | required | Mount target and options. |

**Children:** None. NFS mounts declare a mount point only.

**Compiler behavior:** Emits an fstab entry with `<server>:<export>` as the device, `nfs` or `nfs4` as the type, and assembled options. Adds `_netdev` automatically. If `sec` is any `krb5*` variant, adds `x-systemd.requires=gssproxy.service` (or `rpc-gssd.service` depending on distribution). Requires `nfs-utils` in the image package list.

**SELinux note:** NFS supports SELinux labeling via two mechanisms: (1) `context=` mount option (blanket label, same as `fat32`), and (2) NFSv4.2 labeled NFS (`sec=krb5*` with server-side SELinux support), which transfers file labels from the server. The compiler validates `context=` usage as with other non-xattr filesystems. Labeled NFS is declared by omitting `context` and ensuring `version = 4.2`.

**Validation:**
- `sec = krb5*` variants require Kerberos infrastructure. The compiler emits a warning if no Kerberos keytab is declared in the system definition.
- Soft mounts (`hard = false`) under `strict` security floor or above produce a warning — soft NFS mounts can cause silent data corruption when the server is unreachable.

---

## Mount Expressions

Mount expressions declare where a filesystem is accessible and with what options. They appear as inline property values.

### Syntax

```
mount = <path>
mount = <path> [<option>, <option>, ...]
```

**Examples:**

```
mount = /boot
mount = /boot [nodev, nosuid, noexec]
mount = /home [nodev, nosuid, compress=zstd:3]
mount = /var [nodev, nosuid, noexec, x-systemd.mount-timeout=30]
```

The path is the mount target. Options in brackets are comma-separated and map directly to the `-o` flag of `mount` and the options column of `/etc/fstab`.

**Default mount options:** When no options are specified, the compiler uses `defaults`. The security floor (configurable per-compilation) may inject additional options. For example, a `strict` security floor might automatically add `nodev, nosuid` to all mounts except `/` and `/boot`.

### Fstab Generation

The compiler generates a complete `/etc/fstab` from all declared mount expressions. Filesystem identification uses UUID (obtained after `mkfs` execution at install time). The `dump` and `pass` fields are automatically set:

* `pass = 1` for `/`
* `pass = 2` for all other filesystems
* `pass = 0` for swap, `nfs`, and `tmpfs`
* `dump = 0` for all entries (dump is effectively dead tooling)

### `mount` Block (Extended Form)

When mount configuration is complex enough that inline syntax becomes unwieldy, a `mount` block can replace the inline expression. This is optional — the inline form is preferred when it suffices.

```
ext4 data {
    size = 500G
    
    mount {
        target = /srv/data
        options = [nodev, nosuid, noexec]
        automount = false
        timeout = 30
        requires = [network-online.target]
    }
}
```

**Extended mount properties:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `target` | path | required | Mount point. |
| `options` | array of strings | `[defaults]` | Mount options. |
| `automount` | `true` \| `false` | `true` | Whether to mount at boot. When `false`, generates `noauto` in fstab. |
| `timeout` | integer | none | Mount timeout in seconds. Emits `x-systemd.mount-timeout=` for systemd or equivalent for s6. |
| `requires` | array of strings | none | Systemd units that must be active before mounting. |
| `before` | array of strings | none | Systemd units this mount must complete before. |
| `context` | SELinux context expression | none | SELinux security context for the mount. See [SELinux Context on Mount Expressions](#selinux-context-on-mount-expressions). |
| `fscontext` | SELinux context expression | none | SELinux context for the filesystem superblock object. |
| `defcontext` | SELinux context expression | none | SELinux default context for unlabeled files. |
| `rootcontext` | SELinux context expression | none | SELinux context for the root inode before the filesystem is visible. |

---

## SELinux Context on Mount Expressions

SELinux labels on mount points are the trust boundary between the storage layer and the policy layer. A mislabeled mount under MLS is not a cosmetic defect — it is a policy breach that either denies all access or silently grants access at the wrong sensitivity level. The storage syntax carries label information so the compiler can emit correctly labeled mounts and validate them at compile time.

### Context Expression Syntax

An SELinux context expression is a structured tuple of four (targeted/strict) or five (MLS) colon-separated fields:

```
<user>:<role>:<type>:<range>
```

Where `<range>` is a sensitivity-category expression:

```
s0                          # single sensitivity, no categories
s0:c0.c1023                 # single sensitivity, category range
s0-s15:c0.c1023             # sensitivity range with category range
s0-s3:c0,c5,c12             # sensitivity range with discrete categories
```

The four mount-level context properties map to the four SELinux mount options:

| Ironclad property | Linux mount option | Behavior |
| --- | --- | --- |
| `context` | `context=` | Labels all files and the filesystem itself. Overrides all on-disk xattr labels. Mutually exclusive with the other three. |
| `fscontext` | `fscontext=` | Labels the filesystem superblock only. Used with `defcontext` for xattr-capable filesystems that need a non-default superblock label. |
| `defcontext` | `defcontext=` | Default label for files that have no xattr label. Only affects unlabeled files. |
| `rootcontext` | `rootcontext=` | Labels the root inode before the filesystem is visible to userspace. Used when the root inode must have a specific label for policy transitions during boot. |

### Inline Form

For the common case where only `context` is needed, the inline mount syntax supports a trailing context clause:

```
mount = /boot/efi [nodev, nosuid, noexec] context system_u:object_r:boot_t:s0
```

This is syntactic sugar for the equivalent extended form. Only `context` is available inline — the other three properties (`fscontext`, `defcontext`, `rootcontext`) require the extended `mount` block.

### Extended Form

```
fat32 efi {
    size = 1G
    type = ef00
    
    mount {
        target = /boot/efi
        options = [nodev, nosuid, noexec]
        context = system_u:object_r:boot_t:s0
    }
}
```

For xattr-capable filesystems that need fine-grained control:

```
ext4 containers {
    size = 200G
    
    mount {
        target = /var/lib/containers
        options = [nodev, nosuid]
        defcontext = system_u:object_r:container_var_lib_t:s0
        rootcontext = system_u:object_r:container_var_lib_t:s0
    }
}
```

### MLS-Specific Examples

Under MLS, sensitivity ranges become critical. A multi-level mount point serving data across sensitivity levels:

```
ext4 shared_data {
    size = 100G
    
    mount {
        target = /srv/shared
        options = [nodev, nosuid, noexec]
        defcontext = system_u:object_r:shared_content_t:s0-s9:c0.c255
    }
}
```

A single-level mount pinned to a specific classification:

```
xfs classified {
    size = 500G
    
    mount {
        target = /srv/classified
        options = [nodev, nosuid, noexec]
        context = system_u:object_r:classified_content_t:s5:c0.c127
    }
}
```

Subvolumes with per-mount sensitivity isolation:

```
btrfs data {
    size = remaining
    compress = zstd:1
    
    subvol @public {
        mount {
            target = /srv/public
            options = [nodev, nosuid, noexec]
            defcontext = system_u:object_r:public_content_t:s0
        }
    }
    
    subvol @restricted {
        mount {
            target = /srv/restricted
            options = [nodev, nosuid, noexec]
            defcontext = system_u:object_r:restricted_content_t:s3:c0.c127
        }
    }
    
    subvol @toplevel {
        mount {
            target = /srv/toplevel
            options = [nodev, nosuid, noexec]
            defcontext = system_u:object_r:toplevel_content_t:s7:c0.c255
        }
    }
}
```

### Context and `context=` Mutual Exclusivity

When `context` is set, `fscontext`, `defcontext`, and `rootcontext` are invalid — this is how the Linux mount system works. The compiler rejects declarations that set `context` alongside any of the other three. The logic:

- **`context`** = "I want every object under this mount to carry this label, period. Ignore xattrs." This is the correct choice for xattr-incapable filesystems (`fat32`, `ntfs`, `tmpfs`) and for mounts where a blanket label is operationally appropriate.
- **`fscontext` + `defcontext` + `rootcontext`** = "The filesystem supports xattrs and I want labeled files, but I need to override the defaults for unlabeled objects, the superblock, or the root inode." Use these for xattr-capable filesystems (`ext4`, `xfs`, `btrfs`) where `restorecon` or policy file contexts will handle per-file labeling, but the mount-level defaults need to be set for MLS range correctness.

### Filesystem xattr Capability and Enforcement

The compiler knows which filesystem types support SELinux xattr labeling and which do not:

| Filesystem | xattr support | Label strategy |
| --- | --- | --- |
| `ext4` | yes | File contexts via `restorecon`; mount-level `defcontext`/`rootcontext` optional |
| `xfs` | yes | Same as ext4 |
| `btrfs` | yes | Same as ext4 |
| `zfs dataset` | yes (with `xattr = sa` or `xattr = on`) | File contexts via `restorecon`; mount-level `defcontext`/`rootcontext` optional. Requires `xattr` not set to `off`. |
| `stratis` | yes (XFS-backed) | Same as ext4 |
| `fat32` | no | **Must** use `context=` for all labeling |
| `ntfs` | no | **Must** use `context=` for all labeling |
| `swap` | n/a | Labeled via policy, not mount context |
| `tmpfs` | no | **Must** use `context=` for all labeling |
| `nfs` | conditional | NFSv4.2 with labeled NFS transfers labels from server; otherwise **must** use `context=` |

When no xattr support exists and the SELinux mode is `mls`, the compiler enforces that `context` is declared. Without it, files under the mount inherit `unlabeled_t` which MLS policy denies access to — a guaranteed boot failure or data access failure.

---

## SELinux Sensitivity and Category Validation

The compiler validates SELinux context expressions against the system's declared policy parameters. These parameters are defined outside the storage syntax in the system-level SELinux configuration block (see separate specification), but the storage compiler consumes them for validation.

### What the Storage Compiler Validates

1. **Context field count.** A context must have exactly four colon-separated fields: `user:role:type:range`. Three-field contexts (targeted shorthand) are not valid under MLS — the compiler rejects them.

2. **Range syntax.** The `range` field must be a valid MLS range expression:
   - Sensitivity: `s0` through `s<N>` where `N` is the system's declared `max_sensitivity`.
   - Sensitivity range: `s<low>-s<high>` where `low <= high` and both are within bounds.
   - Categories: `c<N>` discrete, `c<low>.c<high>` range, comma-separated combinations.
   - Category values must be within the system's declared `max_category`.

3. **User existence.** The `user` field must reference an SELinux user declared in the policy module list. The compiler cross-references against the system-level SELinux user declarations.

4. **Type existence.** The `type` field must reference a type declared in one of the loaded policy modules. The compiler cross-references against the system-level module manifest.

5. **Role-user validity.** The `role` field must be a role that the declared user is authorized to assume.

6. **Dominance in ranges.** When a mount declares a sensitivity range (`s0-s5`), the compiler verifies that the low sensitivity dominates (is less than or equal to) the high sensitivity. The compiler also verifies that the declared range does not exceed the user's authorized range.

### What the Storage Compiler Does Not Validate

- Per-file type enforcement rules (that is TE policy, not a storage concern).
- Whether the declared type is appropriate for the mount path (that requires policy-level semantic understanding beyond the storage compiler's scope — the SELinux policy domain specification covers this).
- MLS constraint satisfaction beyond range validity (e.g., whether a process at `s3` can write to a filesystem labeled `s5` — that is a runtime policy enforcement question).

### Validation Failure Behavior

All SELinux context validation failures are compile-time **errors**, not warnings. A malformed context will cause a mount failure at boot time — there is no reasonable "warn and continue" behavior. The compiler halts and reports:

- The offending storage block name and mount target
- The invalid context expression
- Which validation rule was violated
- The valid range/set for the violated field (when applicable)

---

## Whole-Disk and Partitionless Layouts

Not every block device needs a partition table.

### Whole-Disk Filesystem

```
disk /dev/sdc {
    label = none
    
    xfs scratch {
        mount = /mnt/scratch
    }
}
```

When `label = none`, the disk has no partition table. Exactly one filesystem child is permitted, and it consumes the entire device. `size`, `index`, `start`, `end`, and `type` are not valid inside this child — the block device *is* the filesystem's backing device.

**Compiler behavior:** Skips `parted` entirely. Emits `mkfs` directly on the raw block device.

### Whole-Disk Encryption

```
disk /dev/sdd {
    label = none
    
    luks2 secure_scratch {
        xfs encrypted_data {
            mount = /mnt/secure
        }
    }
}
```

LUKS2 wrapping the entire device with no partition table. Valid and sometimes desirable for data drives where partition metadata is unnecessary overhead.

---

## Multi-Disk Scenarios

### Separate Boot and Root Drives

```
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
```

### RAID + LVM + Encryption Stack

```
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
```

### MLS Multi-Level Storage Server

A storage layout for a system handling data at multiple sensitivity levels, with per-volume SELinux labeling:

```
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
        mount {
            target = /boot
            options = [nodev, nosuid, noexec]
            defcontext = system_u:object_r:boot_t:s0
        }
    }
}

disk /dev/nvme0n1 {
    label = gpt
    
    luks2 system {
        index = 1
        size = remaining
        tpm2 = true
        integrity = hmac-sha256
        
        lvm vg_system {
            ext4 root {
                size = 50G
                mount {
                    target = /
                    defcontext = system_u:object_r:default_t:s0-s15:c0.c1023
                }
            }
            
            ext4 var {
                size = 30G
                mount {
                    target = /var
                    options = [nodev, nosuid, noexec]
                    defcontext = system_u:object_r:var_t:s0-s15:c0.c1023
                }
            }
            
            xfs unclassified {
                size = 200G
                mount {
                    target = /srv/unclassified
                    options = [nodev, nosuid, noexec]
                    defcontext = system_u:object_r:public_content_t:s0
                }
            }
            
            xfs confidential {
                size = 500G
                mount {
                    target = /srv/confidential
                    options = [nodev, nosuid, noexec]
                    defcontext = system_u:object_r:confidential_content_t:s4:c0.c255
                }
            }
            
            xfs secret {
                size = 500G
                mount {
                    target = /srv/secret
                    options = [nodev, nosuid, noexec]
                    defcontext = system_u:object_r:secret_content_t:s8:c0.c255
                }
            }
            
            xfs topsecret {
                size = 500G
                mount {
                    target = /srv/topsecret
                    options = [nodev, nosuid, noexec]
                    defcontext = system_u:object_r:topsecret_content_t:s12:c0.c255
                }
            }
            
            swap swap0 { size = 32G }
        }
    }
}
```

### ZFS NAS with Mixed Vdevs

```
zpool storage {
    ashift = 12
    autotrim = true
    
    vdev data-raidz2 {
        type = raidz2
        members = [/dev/sda, /dev/sdb, /dev/sdc, /dev/sdd, /dev/sde, /dev/sdf]
    }
    
    vdev slog {
        type = mirror
        members = [/dev/nvme0n1p1, /dev/nvme1n1p1]
    }
    
    vdev l2arc {
        type = cache
        members = [/dev/nvme0n1p2]
    }
    
    dataset media {
        mountpoint = /srv/media
        compression = zstd
        recordsize = 1M
        atime = false
        exec = false
        setuid = false
        devices = false
    }
    
    dataset containers {
        mountpoint = /var/lib/containers
        compression = zstd
        recordsize = 128K
        quota = 500G
        
        dataset volumes {
            mountpoint = /var/lib/containers/storage/volumes
            reservation = 100G
        }
    }
    
    dataset backups {
        mountpoint = /srv/backups
        compression = zstd:9
        copies = 2
        exec = false
        setuid = false
        devices = false
    }
    
    zvol swap {
        size = 16G
        compression = lz4
        volblocksize = 4K
    }
}
```

### Stratis Managed Application Storage

```
disk /dev/nvme0n1 {
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
        size = 100G
        tpm2 = true
        
        lvm vg0 {
            btrfs root {
                size = remaining
                compress = zstd:1
                
                subvol @ { mount = / }
                subvol @home { mount = /home [nodev, nosuid] }
                subvol @var { mount = /var [nodev, nosuid, noexec] }
            }
            
            swap swap0 { size = 16G }
        }
    }
}

stratis appdata {
    disks = [/dev/sda, /dev/sdb]
    encrypted = true
    cache = [/dev/nvme0n1p4]
    
    filesystem web {
        mountpoint = /srv/www
        size_limit = 200G
    }
    
    filesystem database {
        mountpoint = /var/lib/postgres
        size_limit = 500G
    }
    
    filesystem objects {
        mountpoint = /srv/objects
        size_limit = 1T
    }
}
```

### SAN-Attached Multipath with iSCSI

```
iscsi san_target0 {
    target = "iqn.2024.com.storage:array01.lun0"
    portal = "10.0.1.50:3260"
    portals = ["10.0.2.50:3260"]
    auth = chap
    username = "initiator01"
    multipath = true
}

multipath san_lun0 {
    wwid = "3600508b4000c4a37000009000012a000"
    policy = service-time
    no_path_retry = 12
    
    path /dev/sda { priority_group = 1 }
    path /dev/sdb { priority_group = 1 }
    path /dev/sdc { priority_group = 2 }
    path /dev/sdd { priority_group = 2 }
    
    luks2 encrypted_san {
        cipher = aes-xts-plain64
        key_size = 512
        tpm2 = true
        
        lvm vg_san {
            vdo dedup_archive {
                size = 2T
                virtual_size = 8T
                deduplication = true
                compression = true
                
                xfs archive {
                    mount = /srv/archive [nodev, nosuid, noexec]
                }
            }
            
            xfs hot_data {
                size = 500G
                mount = /srv/data [nodev, nosuid]
            }
        }
    }
}
```

### Hardened System with tmpfs and Integrity

```
tmpfs tmp {
    mount = /tmp [nodev, nosuid, noexec] context system_u:object_r:tmp_t:s0
    size = 2G
}

tmpfs devshm {
    mount = /dev/shm [nodev, nosuid, noexec]
    size = 1G
}

tmpfs run {
    mount = /run [nodev, nosuid] context system_u:object_r:var_run_t:s0
}

disk /dev/nvme0n1 {
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
    
    luks2 system {
        index = 3
        size = remaining
        tpm2 = true
        integrity = hmac-sha256
        
        lvm vg0 {
            ext4 root { size = 50G; mount = / }
            ext4 var { size = 30G; mount = /var [nodev, nosuid, noexec] }
            ext4 home { size = 100G; mount = /home [nodev, nosuid] }
            ext4 audit { size = 20G; mount = /var/log/audit [nodev, nosuid, noexec] }
            swap swap0 { size = 16G }
        }
    }
}

nfs home_nfs {
    server = "nfs.internal.example.com"
    export = "/exports/shared"
    version = 4.2
    sec = krb5p
    
    mount {
        target = /mnt/shared
        options = [nodev, nosuid, noexec, hard, intr]
        requires = [network-online.target, gssproxy.service]
        context = system_u:object_r:nfs_t:s0
    }
}
```

---

## Explicit Partition Positioning

Ironclad prefers `size` over explicit `start`/`end` boundaries. The compiler calculates optimal alignment and placement automatically. However, for operators who need precise control — unusual hardware, pre-existing partition schemes, mixed-use disks — explicit positioning is available.

```
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
```

`end = -1` means end of disk.

**Mixing `size` and `start`/`end`:** Permitted. The compiler resolves explicit boundaries first, then allocates `size`-based partitions in the remaining gaps. `remaining` consumes whatever space is left after all explicit allocations.

**Validation:** The compiler rejects overlapping boundaries, gaps that result in unreachable space (unless intentional via a `raw` block), and `start`/`end` values that exceed the device's reported capacity (when detectable at compile time).

---

## Compiler Validation Rules

The compiler applies the following validation rules to storage declarations. All violations are compile-time errors unless noted as warnings.

### Structural Validation

* Every `mount` target path must be unique across the entire source tree. Duplicate mount points are an error.
* Only one `remaining` size is permitted per parent scope.
* `start`/`end` partitions must not overlap.
* A `disk` with `label = none` must have exactly one filesystem child.
* A `luks2` block without a child `lvm` may contain at most one filesystem child (LUKS opens to a single block device).
* `subvol` blocks are only valid inside `btrfs`.
* `thin` and `vdo` blocks are only valid inside `lvm`.
* `index` values within a `disk` must be unique and positive.
* `mdraid` member disks must not appear in more than one array.
* `zpool` vdev members must not appear in more than one vdev within the same pool.
* `zpool` vdev member counts must meet minimums for their type: `mirror` ≥ 2, `raidz1` ≥ 3, `raidz2` ≥ 4, `raidz3` ≥ 5.
* `dataset` blocks are only valid inside `zpool` or other `dataset` blocks.
* `zvol` blocks are only valid inside `zpool`.
* `vdev` blocks are only valid inside `zpool`.
* `filesystem` (Stratis) blocks are only valid inside `stratis`.
* `path` blocks are only valid inside `multipath`.
* `multipath` `wwid` must be unique across all multipath declarations.
* `nfs` blocks cannot contain children other than a `mount` block or inline mount expression.
* `iscsi` `target` + `portal` combinations must be unique.
* `integrity` blocks inside `luks2` are invalid — use the `integrity` property on the LUKS block instead.
* `tmpfs` blocks must declare a `mount`.
* `stratis` pools with `encrypted = true` must have a `key_desc` or use the pool name as default.

### Capacity Validation

* The sum of `size` values for thick LVM logical volumes must not exceed the parent volume group's available physical extents. **Error.**
* Thin pool virtual allocation exceeding `overcommit_warn` threshold. **Warning.**
* Thin pool virtual allocation exceeding `overcommit_deny` threshold. **Error.**
* VDO `virtual_size` must be greater than or equal to `size`. **Error.**
* VDO physical `size` must be at least `5G`. **Error.**
* VDO `slab_size` must be a power of 2 between `128M` and `32G`. **Error.**
* VDO virtual allocation exceeding `overcommit_warn` threshold. **Warning.**
* `start`/`end` values exceeding device capacity (when detectable). **Error.**
* `zpool` `zvol` total `size` declarations should not exceed total pool capacity (best-effort estimate accounting for vdev overhead). **Warning** when exceeding 80%; **Error** at 100%.
* `stratis` `filesystem` `size_limit` declarations exceeding total pool capacity. **Warning.**

### SELinux Context Validation

* `context` is mutually exclusive with `fscontext`, `defcontext`, and `rootcontext`. Declaring both is an **error**.
* Every context expression must have exactly four colon-separated fields (`user:role:type:range`). Three-field contexts are an **error** under MLS.
* Sensitivity values must be within the system's declared `max_sensitivity`. Out-of-range sensitivities are an **error**.
* Category values must be within the system's declared `max_category`. Out-of-range categories are an **error**.
* In a sensitivity range `s<low>-s<high>`, `low` must be less than or equal to `high`. Inverted ranges are an **error**.
* The `user` field must reference a declared SELinux user. Unknown users are an **error**.
* The `type` field must reference a type declared in the loaded policy module manifest. Unknown types are an **error**.
* The `role` field must be authorized for the declared user. Unauthorized roles are an **error**.
* A `fat32` or `ntfs` filesystem with a `mount` declaration under MLS or `strict`+ security floor must have an explicit `context`. Missing context on xattr-incapable filesystems is an **error**.
* An xattr-capable filesystem (`ext4`, `xfs`, `btrfs`) must not use `context` when per-file labeling is expected — this silently overrides all xattr labels. When the security floor is `maximum`, `context` on xattr-capable filesystems is an **error** (use `defcontext`/`rootcontext` instead). At lower security floors, this is a **warning**.

### Security Floor Validation

The compiler enforces a configurable security floor on storage declarations:

* **Baseline:** No enforcement — the operator's declaration is accepted as-is.
* **Standard:** `/boot` must have `nodev, nosuid, noexec`. `/tmp` must have `nodev, nosuid, noexec`. `/home` must have `nodev, nosuid`. `tmpfs` mounts for `/tmp` and `/dev/shm` must have `nodev, nosuid, noexec`. NFS mounts should use `hard` (soft mount produces a **warning**). Warnings for violations. Under MLS: warnings for mounts without SELinux context declarations.
* **Strict:** Standard rules as errors, not warnings. Root filesystem must be on an encrypted backing device (`luks2` ancestor). Swap must be on an encrypted backing device. `tmpfs` blocks without SELinux `context` are **errors** under MLS. iSCSI connections should use CHAP authentication — `auth = none` produces a **warning**. NFS mounts should use `sec = krb5i` or above — `sec = sys` produces a **warning**. Under MLS: xattr-incapable filesystems must have explicit `context`. All mounts should have either `context` or `defcontext` — missing labels are **warnings**.
* **Maximum:** Strict rules plus: all non-root mounts must have `nodev`. All mounts except `/` and `/boot` must have `nosuid`. All data-only mounts must have `noexec`. iSCSI connections must use CHAP — `auth = none` is an **error**. NFS mounts must use `sec = krb5p` — anything less is an **error**. Under MLS: **every** mount must have an explicit SELinux context declaration (`context` for non-xattr, `defcontext` or `rootcontext` for xattr-capable). Missing labels are **errors**. `context` on xattr-capable filesystems is an **error** (must use `defcontext`/`rootcontext` to preserve per-file labeling).

The security floor level is declared outside the storage block (system-level configuration).

---

## Semicolon Shorthand

For simple declarations where a block contains only a few properties, the semicolon-separated inline form avoids unnecessary vertical space:

```
ext4 root { size = 50G; mount = / }
swap swap0 { size = 16G }
```

This is syntactically identical to the expanded multi-line form. The compiler makes no distinction. The convention is: use inline form for blocks with three or fewer simple properties; expand to multi-line for anything more complex.

**Note:** Inline mount expressions with a trailing `context` clause remain valid in shorthand:

```
fat32 efi { size = 1G; type = ef00; mount = /boot/efi [nodev, nosuid, noexec] context system_u:object_r:boot_t:s0 }
```

However, this approaches the complexity threshold where the extended form is more readable.

---

## Reserved Keywords

The following words are reserved in storage context and cannot be used as block names:

`disk`, `mdraid`, `zpool`, `vdev`, `dataset`, `zvol`, `stratis`, `filesystem`, `multipath`, `path`, `iscsi`, `nfs`, `luks2`, `luks1`, `integrity`, `lvm`, `thin`, `vdo`, `ext4`, `xfs`, `btrfs`, `fat32`, `swap`, `ntfs`, `tmpfs`, `raw`, `subvol`, `mount`, `remaining`, `none`, `whole`, `true`, `false`, `context`, `fscontext`, `defcontext`, `rootcontext`

---

## Grammar Summary (Informative)

This section provides an informal summary of the storage grammar for readability. The canonical grammar is the PEG definition in the compiler source.

```
storage_decl    = (disk_block | mdraid_block | zpool_block | stratis_block
                | multipath_block | iscsi_block | nfs_block | tmpfs_block)*

disk_block      = "disk" device_path "{" disk_body "}"
disk_body       = property* (partition_block | fs_block | luks_block)*

mdraid_block    = "mdraid" name "{" mdraid_body "}"
mdraid_body     = property* (fs_block | luks_block | lvm_block)*

zpool_block     = "zpool" name "{" property* (vdev_block | dataset_block | zvol_block)* "}"
vdev_block      = "vdev" name "{" property* "}"
dataset_block   = "dataset" name "{" property* dataset_block* "}"
zvol_block      = "zvol" name "{" property* (swap_block | fs_block | luks_block)* "}"

stratis_block   = "stratis" name "{" property* stratis_fs_block* "}"
stratis_fs_block = "filesystem" name "{" property* "}"

multipath_block = "multipath" name "{" property* path_block* (fs_block | luks_block | lvm_block | raw_block)* "}"
path_block      = "path" device_path "{" property* "}"

iscsi_block     = "iscsi" name "{" property* (fs_block | luks_block | lvm_block | raw_block)* "}"
nfs_block       = "nfs" name "{" property* mount_block "}"
tmpfs_block     = "tmpfs" name "{" property* "}"

partition_block = (fs_keyword | "luks2" | "luks1" | "raw") name "{" partition_body "}"
partition_body  = property* child_block*

luks_block      = ("luks2" | "luks1") name "{" property* (fs_block | lvm_block) "}"
integrity_block = "integrity" name "{" property* (fs_block | lvm_block | swap_block) "}"

lvm_block       = "lvm" name "{" property* (fs_block | swap_block | thin_block | vdo_block)* "}"
thin_block      = "thin" name "{" property* (fs_block | swap_block)* "}"
vdo_block       = "vdo" name "{" property* (fs_block | swap_block) "}"

fs_block        = fs_keyword name "{" property* subvol_block* "}"
fs_keyword      = "ext4" | "xfs" | "btrfs" | "fat32" | "ntfs"

swap_block      = "swap" name "{" property* "}"
subvol_block    = "subvol" name "{" property* "}"

raw_block       = "raw" name "{" property* "}"

property        = identifier "=" value
value           = string | number | size | boolean | array | identifier
                | selinux_context

mount_expr      = path ( "[" option ("," option)* "]" )? ( "context" selinux_context )?
mount_block     = "mount" "{" mount_property* "}"
mount_property  = "target" "=" path
                | "options" "=" "[" option ("," option)* "]"
                | "automount" "=" boolean
                | "timeout" "=" integer
                | "requires" "=" "[" string ("," string)* "]"
                | "before" "=" "[" string ("," string)* "]"
                | "context" "=" selinux_context
                | "fscontext" "=" selinux_context
                | "defcontext" "=" selinux_context
                | "rootcontext" "=" selinux_context

selinux_context = selinux_user ":" selinux_role ":" selinux_type ":" mls_range
mls_range       = sensitivity ( "-" sensitivity )? ( ":" category_set )?
sensitivity     = "s" digit+
category_set    = category_expr ( "," category_expr )*
category_expr   = "c" digit+ ( "." "c" digit+ )?

size            = number unit | percentage | "remaining"
unit            = "B" | "K" | "KB" | "M" | "MB" | "G" | "GB" | "T" | "TB"
```

---

## What This Document Does Not Cover

This specification covers storage declaration syntax only. The following topics are defined in separate specifications:

* **Class system and inheritance** — How storage declarations compose with classes
* **Variables, loops, and conditionals** — Parameterizing storage layouts across fleet roles
* **Kernel, init, services, users, SELinux policy, firewall** — Other system declaration domains
* **SELinux policy modules and type enforcement** — Declaring SELinux users, roles, types, modules, and TE rules (the storage syntax only consumes these declarations for validation — it does not define them)
* **SELinux file contexts** — Per-path labeling rules applied by `restorecon` (separate from the mount-level context declarations in this specification)
* **Compiler output mapping** — How declarations map to Kickstart, Ansible, and other backends
* **Runtime agent storage monitoring** — How drift detection applies to storage state, including SELinux label drift on mount points
* **Secret management** — LUKS passphrase generation, distribution, and escrow
