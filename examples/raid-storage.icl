# RAID + LVM + Encryption stack
# Mirror boot on RAID1, data on RAID10 with LUKS2 and thin provisioning

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
