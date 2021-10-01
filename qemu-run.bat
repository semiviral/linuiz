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
    -d guest_errors,trace:pci_nvme_err_invalid_dma,trace:pci_nvme_err_invalid_prplist_ent,trace:pci_nvme_err_invalid_prp2_align,trace:pci_nvme_err_invalid_prp2_missing,trace:pci_nvme_err_invalid_prp,trace:pci_nvme_err_invalid_ns,trace:pci_nvme_err_invalid_opc,trace:pci_nvme_err_invalid_admin_opc,trace:pci_nvme_err_invalid_lba_range,trace:pci_nvme_err_invalid_del_sq,trace:pci_nvme_err_invalid_create_sq_cqid,trace:pci_nvme_err_invalid_create_sq_sqid,trace:pci_nvme_err_invalid_create_sq_size,trace:pci_nvme_err_invalid_create_sq_addr,trace:pci_nvme_err_invalid_create_sq_qflags,trace:pci_nvme_err_invalid_del_cq_cqid,trace:pci_nvme_err_invalid_del_cq_notempty,trace:pci_nvme_err_invalid_create_cq_cqid,trace:pci_nvme_err_invalid_create_cq_size,trace:pci_nvme_err_invalid_create_cq_addr,trace:pci_nvme_err_invalid_create_cq_vector,trace:pci_nvme_err_invalid_create_cq_qflags,trace:pci_nvme_err_invalid_identify_cns,trace:pci_nvme_err_invalid_getfeat,trace:pci_nvme_err_invalid_setfeat^
    -D qemu_debug.log^