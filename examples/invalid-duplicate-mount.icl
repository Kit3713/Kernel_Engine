# Invalid: duplicate mount targets — should produce a validation error

disk /dev/sda {
    label = gpt

    ext4 data1 {
        index = 1
        size = 100G
        mount = /data
    }

    ext4 data2 {
        index = 2
        size = 100G
        mount = /data
    }
}
