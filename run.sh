#!/bin/bash

qemu-system-x86_64 \
    -no-reboot \
    -machine q35 \
    -cpu qemu64 \
    -smp 1 \
    -m 64M \
    -serial stdio \
    -display none \
    -net none \
    -bios ./ovmf.fd \
    -drive format=raw,file=fat:rw:./.hdd/image/ \
    -drive format=raw,file=./.hdd/nvme.img,id=nvm,if=none \
    -device nvme,drive=nvm,serial=deadbeef \
    -D .debug/qemu_debug.log \
    -d int,guest_errors
