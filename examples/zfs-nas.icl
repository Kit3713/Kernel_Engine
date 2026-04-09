# ZFS NAS with mirror vdevs, datasets, and zvol

zpool tank {
    ashift = 12
    compression = lz4
    autoexpand = true

    vdev data0 {
        type = mirror
        members = [/dev/sda, /dev/sdb]
    }

    vdev data1 {
        type = mirror
        members = [/dev/sdc, /dev/sdd]
    }

    vdev slog0 {
        type = log
        members = [/dev/nvme0n1p1]
    }

    dataset home {
        mountpoint = /home
        quota = 500G

        dataset alice {
            quota = 100G
        }

        dataset bob {
            quota = 100G
        }
    }

    dataset media {
        mountpoint = /srv/media
        compression = zstd
    }

    zvol iscsi_target {
        size = 200G
        blocksize = 8K

        ext4 iscsi_fs {
            mount = /srv/iscsi
        }
    }
}
