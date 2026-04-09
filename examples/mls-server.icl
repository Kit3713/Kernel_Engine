# MLS Multi-Level Storage Server
# Per-volume SELinux labeling at different sensitivity levels

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
