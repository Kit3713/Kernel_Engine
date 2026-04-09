# Stratis managed storage pool with encrypted backing

stratis appdata {
    disks = [/dev/sda, /dev/sdb]
    encrypted = true

    filesystem databases {
        size_limit = 200G

        mount {
            target = /var/lib/postgres
            options = [nodev, nosuid]
        }
    }

    filesystem objects {
        size_limit = 500G

        mount {
            target = /srv/objects
            options = [nodev, nosuid, noexec]
        }
    }
}
