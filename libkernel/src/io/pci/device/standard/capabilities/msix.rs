use crate::{
    addr_ty::Virtual,
    io::pci::standard::StandardRegister,
    memory::volatile::{Volatile, VolatileCell},
    volatile_bitfield_getter, Address, ReadOnly, ReadWrite,
};
use bit_field::BitField;
use core::{convert::TryFrom, fmt};

#[repr(C)]
pub struct MessageTableEntry {
    msg_addr_low: VolatileCell<u32, ReadOnly>,
    msg_addr_high: VolatileCell<u32, ReadOnly>,
    msg_data: VolatileCell<u32, ReadWrite>,
    mask: VolatileCell<u32, ReadWrite>,
}

impl MessageTableEntry {
    pub fn get_addr(&self) -> Address<Virtual> {
        let addr_low = (self.msg_addr_low.read() & !0b11111) as usize;
        let addr_high = (self.msg_addr_high.read() as usize) << 32;

        Address::<Virtual>::new(addr_high | addr_low)
    }

    pub fn get_message_data(&self) -> u32 {
        self.msg_data.read()
    }

    pub fn set_message_data(&self, value: u32) {
        self.msg_data.write(value);
    }

    volatile_bitfield_getter!(mask, masked, 0);
}

impl Volatile for MessageTableEntry {}

impl fmt::Debug for MessageTableEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Message Table Entry")
            .field(&self.get_masked())
            .field(&self.get_addr())
            .field(&self.get_message_data())
            .finish()
    }
}

#[repr(C)]
pub struct MessageControl {
    reg0: VolatileCell<u32, ReadWrite>,
}

impl MessageControl {
    pub fn get_table_len(&self) -> usize {
        self.reg0.read().get_bits(16..27) as usize
    }

    volatile_bitfield_getter!(reg0, force_mask, 30);
    volatile_bitfield_getter!(reg0, enable, 31);
}

impl fmt::Debug for MessageControl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Message Control")
            .field("Enabled", &self.get_enable())
            .field("Force Mask", &self.get_force_mask())
            .field("Table Size", &self.get_table_len())
            .finish()
    }
}

#[repr(C)]
pub struct MSIX {
    message_control: MessageControl,
    reg1: VolatileCell<u32, ReadOnly>,
    reg2: VolatileCell<u32, ReadOnly>,
}

impl MSIX {
    pub fn message_control(&self) -> &MessageControl {
        &self.message_control
    }

    pub fn get_table_bir(&self) -> StandardRegister {
        StandardRegister::try_from(self.reg1.read().get_bits(0..3) as usize)
            .expect("reserved BIR value")
    }

    pub fn get_table_offset(&self) -> usize {
        (self.reg1.read() & !0b111) as usize
    }

    pub fn get_pending_bit_bir(&self) -> StandardRegister {
        StandardRegister::try_from(self.reg2.read().get_bits(0..3) as usize)
            .expect("reserved pending BIR value")
    }

    pub fn get_pending_bit_offset(&self) -> usize {
        (self.reg2.read() & !0b111) as usize
    }

    pub fn get_message_table<'dev>(
        &self,
        device: &'dev crate::io::pci::PCIeDevice<crate::io::pci::Standard>,
    ) -> Option<&mut [&'dev MessageTableEntry]> {
        device
            .get_register(self.get_table_bir())
            .map(|mmio| unsafe {
                let table_offset = self.get_table_offset();
                let table =
                    crate::slice_mut!(&MessageTableEntry, self.message_control().get_table_len());

                table.iter_mut().enumerate().for_each(|(index, entry)| {
                    *entry = mmio
                        .borrow::<MessageTableEntry>(
                            table_offset + (index * core::mem::size_of::<MessageTableEntry>()),
                        )
                        .unwrap()
                });

                table
            })
    }
}

impl Volatile for MSIX {}

impl fmt::Debug for MSIX {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MSI-X")
            .field("Message Control", &self.message_control())
            .field("BIR", &self.get_table_bir())
            .field("Table Offset", &self.get_table_offset())
            .field("Pending Bit BIR", &self.get_pending_bit_bir())
            .field("Pending Bit Offset", &self.get_pending_bit_offset())
            .finish()
    }
}
