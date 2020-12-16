#![no_std]
#![no_main]
#![feature(asm)]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]

extern crate pic8259_simple;

mod drivers;
mod gdt;
mod instructions;
mod io;
mod pic;

use core::{alloc::Layout, panic::PanicInfo};
use drivers::{
    graphics::{
        color::{Color8i, Colors},
        framebuffer::FramebufferDriver,
    },
    serial::{self, Serial},
};
use efi_boot::{entrypoint, Framebuffer};

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[alloc_error_handler]
fn alloc_error(_error: Layout) -> ! {
    loop {}
}

entrypoint!(kernel_main);
extern "win64" fn kernel_main(framebuffer: Option<Framebuffer>) -> i32 {
    serial::safe_lock(|serial| {
        serial.data_port().write(b'X');
    });

    // let mut framebuffer_driver = FramebufferDriver::new(
    //     framebuffer.unwrap().pointer as *mut Color8i,
    //     0xB71B000 as *mut Color8i,
    //     framebuffer.unwrap().size,
    // );

    // framebuffer_driver.clear(Colors::LightBlue.into(), true);

    loop {}
}
