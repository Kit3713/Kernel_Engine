# SELinux system configuration with storage

disk /dev/sda {
    label = gpt

    fat32 efi {
        index = 1
        size = 1G
        mount = /boot/efi [nodev, nosuid, noexec] context system_u:object_r:boot_t:s0
    }

    luks2 system {
        index = 2
        size = remaining
        tpm2 = true

        lvm vg0 {
            ext4 root {
                size = 50G
                mount = /
            }

            ext4 containers {
                size = 200G

                mount {
                    target = /var/lib/containers
                    options = [nodev, nosuid]
                    defcontext = system_u:object_r:container_var_lib_t:s0
                    rootcontext = system_u:object_r:container_var_lib_t:s0
                }
            }

            swap swap0 {
                size = 16G
            }
        }
    }
}

selinux {
    mode = enforcing
    policy = targeted
    floor = standard

    user system_u {
        roles = [system_r, unconfined_r]
        level = s0-s0:c0.c1023
    }

    user staff_u {
        roles = [staff_r, sysadm_r]
        level = s0-s0:c0.c1023
        default = true
    }

    user sysadm_u {
        roles = [sysadm_r]
        level = s0-s0:c0.c1023
    }

    role sysadm_r {
        types = [sysadm_t]
        level = s0-s0:c0.c1023
    }

    role staff_r {
        types = [staff_t]
        level = s0-s0:c0.c1023
    }

    booleans {
        httpd_can_network_connect = true
        container_manage_cgroup = true
        virt_use_nfs = false
    }
}
