# Hardened server storage layout
# EFI + boot on SSD, encrypted root on NVMe

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
        cipher = aes-xts-plain64
        key_size = 512
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
