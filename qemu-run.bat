qemu-system-x86_64^
    -m 4G^
    -serial stdio^
    -machine q35^
    -cpu qemu64^
    -drive format=raw,file=fat:rw:./hdd/image/^
    -drive if=pflash,format=raw,unit=0,file=./ovmf/OVMF_CODE-pure-efi.fd,readonly=on^
    -drive if=pflash,format=raw,unit=1,file=./ovmf/OVMF_VARS-pure-efi.fd,readonly=on^
    -drive id=disk,if=none,file=./hdd/rootfs.img^
    -device ahci,id=ahci^
    -device ide-hd,drive=disk,bus=ahci.0^
    -drive file=./hdd/nvme.img,if=none,id=nvm^
    -device nvme,drive=nvm,serial=deadbeef^
    -net none^
