# dm-integrity and LVM VDO example

disk /dev/sda {
    label = gpt

    fat32 efi {
        index = 1
        size = 1G
        type = ef00
        mount = /boot/efi [nodev, nosuid, noexec]
    }

    integrity secured_boot {
        index = 2
        size = 2G
        algorithm = crc32c

        ext4 boot {
            mount = /boot [nodev, nosuid, noexec]
        }
    }

    luks2 system {
        index = 3
        size = remaining
        cipher = aes-xts-plain64
        tpm2 = true

        lvm vg_system {
            ext4 root {
                size = 50G
                mount = /
            }

            vdo archive {
                size = 100G
                virtual_size = 500G
                deduplication = true
                compression = true

                xfs dedup_store {
                    mount = /srv/archive [nodev, nosuid, noexec]
                }
            }

            swap swap0 {
                size = 16G
            }
        }
    }
}
