# NFS + tmpfs configuration

nfs home_share {
    server = fileserver.corp.local
    export = /exports/home
    version = 4.2

    mount {
        target = /home
        options = [nodev, nosuid, "sec=krb5p"]
        automount = true
        timeout = 60
        requires = [network-online.target, nfs-client.target]
    }
}

nfs media_share {
    server = media.corp.local
    export = /exports/media
    version = 4.2

    mount {
        target = /srv/media
        options = [nodev, nosuid, noexec, ro]
    }
}

tmpfs runtime {
    size = 2G
    mode = 1777

    mount {
        target = /tmp
        options = [nodev, nosuid, noexec]
    }
}

tmpfs shm {
    size = 4G

    mount {
        target = /dev/shm
        options = [nodev, nosuid, noexec]
    }
}
