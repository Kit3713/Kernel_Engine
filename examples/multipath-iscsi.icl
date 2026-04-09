# Multipath SAN + iSCSI remote storage

multipath san0 {
    wwid = "3600508b1001c5a7380000300000e0000"
    policy = round-robin

    path /dev/sda {
        priority = 1
    }

    path /dev/sdb {
        priority = 2
    }

    luks2 encrypted_san {
        cipher = aes-xts-plain64
        key_size = 512
        tpm2 = true

        lvm vg_san {
            xfs san_data {
                size = remaining

                mount {
                    target = /srv/san
                    options = [nodev, nosuid]
                }
            }
        }
    }
}

iscsi remote_db {
    target = iqn.2024.com.example:db-storage
    portal = 192.168.1.100
    auth = chap
    username = "db_user"

    luks2 encrypted_remote {
        cipher = aes-xts-plain64

        ext4 remote_data {
            mount = /var/lib/mysql [nodev, nosuid]
        }
    }
}
