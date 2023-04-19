mod params;

pub fn get_parameters() -> &'static params::Parameters {
    params::PARAMETERS.get().expect("parameters have not yet been parsed")
}

use crate::memory::{alloc::pmm::PMM, paging::PageDepth};
use libkernel::LinkerSymbol;
use libsys::{page_size, Address};

pub static KERNEL_HANDLE: spin::Lazy<uuid::Uuid> = spin::Lazy::new(uuid::Uuid::new_v4);

#[macro_export]
macro_rules! call_once {
    () => {};

    ($vis:vis fn $name:ident($($arg:tt)*) $body:block) => {
        crate::call_once!($vis fn $name($($arg)*) -> () $body);
    };

    ($vis:vis fn $name:ident($($arg:tt)*) -> $t:ty $body:block) => {
        $vis fn $name($($arg)*) -> $t {
            use core::sync::atomic::{AtomicBool, Ordering};

            static CALLED: AtomicBool = AtomicBool::new(false);

            if CALLED.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_ok() {
                    $body
            } else {
                panic!("{} called more than once", stringify!($name));
            }
        }
    };
}

/// ### Safety
///
/// This function should only ever be called by the bootloader.
#[no_mangle]
#[doc(hidden)]
#[allow(clippy::too_many_lines)]
unsafe extern "C" fn _entry() -> ! {
    setup_logging();

    print_boot_info();

    crate::cpu::setup();

    setup_memory();

    debug!("Initializing ACPI interface...");
    crate::acpi::init_interface();

    load_drivers();

    setup_smp();

    debug!("Reclaiming bootloader memory...");
    crate::boot::reclaim_boot_memory({
        extern "C" {
            static __symbols_start: LinkerSymbol;
            static __symbols_end: LinkerSymbol;
        }

        &[__symbols_start.as_usize()..__symbols_end.as_usize()]
    });
    debug!("Bootloader memory reclaimed.");

    kernel_core_setup()
}

/// ### Safety
///
/// This function should only ever be called once per core.
#[inline(never)]
pub(self) unsafe fn kernel_core_setup() -> ! {
    crate::local::init(1000);

    // Ensure we enable interrupts prior to enabling the scheduler.
    crate::interrupts::enable();
    crate::local::begin_scheduling();

    // This interrupt wait loop is necessary to ensure the core can jump into the scheduler.
    crate::interrupts::wait_loop()
}

call_once!(
    fn setup_logging() {
        if cfg!(debug_assertions) {
            // Logging isn't set up, so we'll just spin loop if we fail to initialize it.
            crate::logging::init().unwrap_or_else(|_| crate::interrupts::wait_loop());
        } else {
            // Logging failed to initialize, but just continue to boot (only in release).
            crate::logging::init().ok();
        }
    }
);

call_once!(
    fn print_boot_info() {
        extern "C" {
            static __build_id: LinkerSymbol;
        }

        #[limine::limine_tag]
        static BOOT_INFO: limine::BootInfoRequest = limine::BootInfoRequest::new(crate::boot::LIMINE_REV);

        // Safety: Symbol is provided by linker script.
        info!("Build ID            {}", unsafe { __build_id.as_usize() });

        if let Some(boot_info) = BOOT_INFO.get_response() {
            info!("Bootloader Info     {} v{} (rev {})", boot_info.name(), boot_info.version(), boot_info.revision());
        } else {
            info!("No bootloader info available.");
        }

        // Vendor strings from the CPU need to be enumerated per-platform.
        #[cfg(target_arch = "x86_64")]
        if let Some(vendor_info) = crate::arch::x64::cpuid::VENDOR_INFO.as_ref() {
            info!("Vendor              {}", vendor_info.as_str());
        } else {
            info!("Vendor              Unknown");
        }
    }
);

call_once!(
    fn setup_memory() {
        {
            #[limine::limine_tag]
            static LIMINE_KERNEL_ADDR: limine::KernelAddressRequest =
                limine::KernelAddressRequest::new(crate::boot::LIMINE_REV);
            #[limine::limine_tag]
            static LIMINE_KERNEL_FILE: limine::KernelFileRequest =
                limine::KernelFileRequest::new(crate::boot::LIMINE_REV);

            extern "C" {
                static KERN_BASE: LinkerSymbol;
            }

            debug!("Preparing kernel memory system.");

            // Extract kernel address information.
            let (kernel_phys_addr, kernel_virt_addr) = LIMINE_KERNEL_ADDR
                .get_response()
                .map(|response| {
                    (
                        usize::try_from(response.physical_base()).unwrap(),
                        usize::try_from(response.virtual_base()).unwrap(),
                    )
                })
                .expect("bootloader did not provide kernel address info");

            // Take reference to kernel file data.
            let kernel_file = LIMINE_KERNEL_FILE
                .get_response()
                .map(limine::KernelFileResponse::file)
                .expect("bootloader did not provide kernel file data");

            /* parse parameters */
            params::PARAMETERS.call_once(|| params::Parameters::parse(kernel_file.cmdline()));

            // Safety: Bootloader guarantees the provided information to be correct.
            let kernel_elf = elf::ElfBytes::<elf::endian::AnyEndian>::minimal_parse(kernel_file.data())
                .expect("kernel file is not a valid ELF");

            /* load and map segments */

            crate::memory::with_kmapper(|kmapper| {
                use crate::memory::{paging::TableEntryFlags, Hhdm};
                use limine::MemoryMapEntryType;

                const PT_FLAG_EXEC_BIT: usize = 0;
                const PT_FLAG_WRITE_BIT: usize = 1;

                /* load kernel segments */
                kernel_elf
                    .segments()
                    .expect("kernel file has no segments")
                    .into_iter()
                    .filter(|ph| ph.p_type == elf::abi::PT_LOAD)
                    .for_each(|phdr| {
                        use bit_field::BitField;

                        debug!("{:X?}", phdr);

                        // Safety: `KERNEL_BASE` is a linker symbol to an in-executable memory location, so it is guaranteed to
                        //         be valid (and is never written to).
                        let base_offset = usize::try_from(phdr.p_vaddr).unwrap() - unsafe { KERN_BASE.as_usize() };
                        let offset_end = base_offset + usize::try_from(phdr.p_memsz).unwrap();
                        let page_attributes = {
                            if phdr.p_flags.get_bit(PT_FLAG_EXEC_BIT) {
                                TableEntryFlags::RX
                            } else if phdr.p_flags.get_bit(PT_FLAG_WRITE_BIT) {
                                TableEntryFlags::RW
                            } else {
                                TableEntryFlags::RO
                            }
                        };

                        (base_offset..offset_end)
                            .step_by(page_size())
                            // Tuple the memory offset to the respect physical and virtual addresses.
                            .map(|mem_offset| {
                                (
                                    Address::new(kernel_phys_addr + mem_offset).unwrap(),
                                    Address::new(kernel_virt_addr + mem_offset).unwrap(),
                                )
                            })
                            // Attempt to map the page to the frame.
                            .try_for_each(|(paddr, vaddr)| {
                                trace!("Map   paddr: {:X?}   vaddr: {:X?}   attrs {:?}", paddr, vaddr, page_attributes);
                                kmapper.map(vaddr, PageDepth::min(), paddr, false, page_attributes)
                            })
                            .expect("failed to map kernel segments");
                    });

                /* map the higher-half direct map */
                debug!("Mapping the higher-half direct map.");
                crate::boot::get_memory_map()
                    .expect("bootloader memory map is required to map HHDM")
                    .iter()
                    // Filter bad memory, or provide the entry's page attributes.
                    .filter_map(|entry| {
                        match entry.ty() {
                    MemoryMapEntryType::Usable
                            | MemoryMapEntryType::AcpiNvs
                            | MemoryMapEntryType::AcpiReclaimable
                            | MemoryMapEntryType::BootloaderReclaimable
                            // TODO handle the PATs or something to make this WC
                            | MemoryMapEntryType::Framebuffer => Some((entry, TableEntryFlags::RW)),

                            MemoryMapEntryType::Reserved | MemoryMapEntryType::KernelAndModules => {
                                Some((entry, TableEntryFlags::RO))
                            }

                            MemoryMapEntryType::BadMemory => None,
                        }
                    })
                    // Flatten the enumeration of every page in the entry.
                    .flat_map(|(entry, attributes)| {
                        entry
                            .range()
                            .step_by(page_size())
                            .map(move |phys_base| (phys_base.try_into().unwrap(), attributes))
                    })
                    // Attempt to map each of the entry's pages.
                    .try_for_each(|(phys_base, attributes)| {
                        kmapper.map(
                            Address::new_truncate(Hhdm::address().get() + phys_base),
                            PageDepth::min(),
                            Address::new_truncate(phys_base),
                            false,
                            attributes,
                        )
                    })
                    .expect("failed mapping the HHDM");

                /* map architecture-specific memory */
                debug!("Mapping the architecture-specific memory.");
                #[cfg(target_arch = "x86_64")]
                {
                    let apic_address = msr::IA32_APIC_BASE::get_base_address().try_into().unwrap();
                    kmapper
                        .map(
                            Address::new_truncate(Hhdm::address().get() + apic_address),
                            PageDepth::min(),
                            Address::new_truncate(apic_address),
                            false,
                            TableEntryFlags::MMIO,
                        )
                        .unwrap();
                }

                debug!("Switching to kernel page tables...");
                // Safety: Kernel mappings should be identical to the bootloader mappings.
                unsafe { kmapper.swap_into() };
                debug!("Kernel has finalized control of page tables.");
            });

            /* load symbols */
            if get_parameters().low_memory {
                debug!("Kernel is running in low memory mode; stack tracing will be disabled.");
            } else if let Ok(Some((symbol_table, string_table))) = kernel_elf.symbol_table() {
                debug!("Loading kernel symbol table...");

                crate::panic::KERNEL_SYMBOLS.call_once(|| {
                    let symbols_iter = symbol_table
                        .into_iter()
                        .map(|symbol| (string_table.get(symbol.st_name as usize).unwrap_or("Unidentified"), symbol));
                    let vec = alloc::vec::Vec::from_iter(symbols_iter);
                    debug!("Loaded {} kernel symbols.", vec.len());

                    alloc::vec::Vec::leak(vec)
                });
            } else {
                warn!("Failed to load any kernel symbols; stack tracing will be disabled.");
            }
        }
    }
);

// call_once!(
fn load_drivers() {
    use crate::proc::{AddressSpace, EntryPoint, Priority, Process, DEFAULT_USERSPACE_SIZE};
    use elf::{endian::AnyEndian, ElfBytes};

    #[limine::limine_tag]
    static LIMINE_MODULES: limine::ModuleRequest = limine::ModuleRequest::new(crate::boot::LIMINE_REV);

    debug!("Unpacking kernel drivers...");

    let Some(modules) = LIMINE_MODULES.get_response() else {
            warn!("Bootloader provided no modules; skipping driver loading.");
            return;
        };
    trace!("{:?}", modules);

    let modules = modules.modules();
    trace!("Found modules: {:X?}", modules);

    let Some(drivers_module) = modules.iter().find(|module| module.path().ends_with("drivers")) else {
            warn!("No drivers module found; skipping driver loading.");
            return;
        };

    let archive = tar_no_std::TarArchiveRef::new(drivers_module.data());
    archive
        .entries()
        .into_iter()
        .filter_map(|entry| {
            debug!("Attempting to parse driver blob: {}", entry.filename());

            let Ok(elf) = ElfBytes::<AnyEndian>::minimal_parse(entry.data()) else {
                warn!("Failed to parse driver blob into ELF");
                return None
            };

            trace!("Driver blob is ELF: {:X?}", elf.ehdr);
            Some(elf)
        })
        .for_each(|elf| {
            let phdrs = elf.segments().expect("driver blob has no loadable segments.");
            let entry_point = unsafe { core::mem::transmute::<_, EntryPoint>(elf.ehdr.e_entry) };
            let mut address_space = AddressSpace::new(
                DEFAULT_USERSPACE_SIZE,
                unsafe {
                    crate::memory::mapper::Mapper::new_unsafe(
                        PageDepth::current(),
                        crate::memory::copy_kernel_page_table().unwrap(),
                    )
                },
                &*PMM,
            );

            for phdr in phdrs.iter().filter(|phdr| phdr.p_type == elf::abi::PT_LOAD) {
                use crate::proc::{MmapFlags, PT_FLAG_EXEC_BIT, PT_FLAG_WRITE_BIT};
                use bit_field::BitField;

                trace!("Processing segment: {:?}", phdr);

                let size_offset = usize::try_from(phdr.p_vaddr).unwrap() & libsys::page_mask();
                let total_size = size_offset + usize::try_from(phdr.p_memsz).unwrap();
                let page_count = libsys::align_up_div(total_size, libsys::page_shift());
                let segment_vaddr = Address::new_truncate(phdr.p_vaddr as usize);
                let page_count = core::num::NonZeroUsize::new(page_count.try_into().unwrap()).unwrap();

                address_space
                    .mmap(Some(segment_vaddr), page_count, MmapFlags::NOT_DEMAND | MmapFlags::READ_WRITE)
                    .unwrap();

                let segment_data = elf.segment_data(&phdr).unwrap();
                unsafe {
                    trace!("Copying elf data...");
                    core::ptr::copy_nonoverlapping(segment_data.as_ptr(), segment_vaddr.as_ptr(), segment_data.len());

                    if phdr.p_memsz > phdr.p_filesz {
                        trace!("Zeroing elf data...");
                        core::ptr::write_bytes(
                            segment_vaddr.as_ptr().add(segment_data.len()),
                            0x0,
                            usize::try_from(phdr.p_memsz - phdr.p_filesz).unwrap(),
                        );
                    }
                }

                address_space
                    .set_mmap_flags(
                        segment_vaddr,
                        page_count,
                        MmapFlags::NOT_DEMAND
                            | if phdr.p_flags.get_bit(PT_FLAG_WRITE_BIT) {
                                MmapFlags::READ_EXECUTE
                            } else if phdr.p_flags.get_bit(PT_FLAG_EXEC_BIT) {
                                MmapFlags::READ_WRITE
                            } else {
                                MmapFlags::empty()
                            },
                    )
                    .unwrap();
            }

            let task = Process::new(Priority::Normal, entry_point, address_space);

            crate::proc::PROCESSES.lock().push_back(task);
        });
}
// );

call_once!(
    fn setup_smp() {
        #[limine::limine_tag]
        static LIMINE_SMP: limine::SmpRequest = limine::SmpRequest::new(crate::boot::LIMINE_REV)
            // Enable x2APIC mode if available.
            .flags(0b1);

        // Safety: `LIMINE_SMP` is only ever accessed within this individual context, and is effectively
        //          dropped as soon as this context goes out of scope.
        let limine_smp = unsafe { &mut *(&raw const LIMINE_SMP).cast_mut() };

        debug!("Detecting and starting additional cores.");

        limine_smp.get_response_mut().map(limine::SmpResponse::cpus).map_or_else(
            || debug!("Bootloader detected no additional CPU cores."),
            // Iterate all of the CPUs, and jump them to the SMP function.
            |cpus| {
                for cpu_info in cpus {
                    trace!("Starting processor: ID P{}/L{}", cpu_info.processor_id(), cpu_info.lapic_id());

                    if get_parameters().smp {
                        extern "C" fn _smp_entry(_: &limine::CpuInfo) -> ! {
                            crate::cpu::setup();

                            // Safety: All currently referenced memory should also be mapped in the kernel page tables.
                            crate::memory::with_kmapper(|kmapper| unsafe { kmapper.swap_into() });

                            // Safety: Function is called only once for this core.
                            unsafe { kernel_core_setup() }
                        }

                        // If smp is enabled, jump to the smp entry function.
                        cpu_info.jump_to(_smp_entry, None);
                    } else {
                        extern "C" fn _idle_forever(_: &limine::CpuInfo) -> ! {
                            // Safety: Murder isn't legal. Is this?
                            unsafe { crate::interrupts::halt_and_catch_fire() }
                        }

                        // If smp is disabled, jump to the park function for the core.
                        cpu_info.jump_to(_idle_forever, None);
                    }
                }
            },
        );
    }
);

/* load driver */

// Push ELF as global task.

// let stack_address = {
//     const TASK_STACK_BASE_ADDRESS: Address<Page> = Address::<Page>::new_truncate(
//         Address::<Virtual>::new_truncate(128 << 39),
//         Some(PageAlign::Align2MiB),
//     );
//     // TODO make this a dynamic configuration
//     const TASK_STACK_PAGE_COUNT: usize = 2;

//     for page in (0..TASK_STACK_PAGE_COUNT)
//         .map(|offset| TASK_STACK_BASE_ADDRESS.forward_checked(offset).unwrap())
//     {
//         driver_page_manager
//             .map(
//                 page,
//                 Address::<Frame>::zero(),
//                 false,
//                 PageAttributes::WRITABLE
//                     | PageAttributes::NO_EXECUTE
//                     | PageAttributes::DEMAND
//                     | PageAttributes::USER
//                     | PageAttributes::HUGE,
//             )
//             .unwrap();
//     }

//     TASK_STACK_BASE_ADDRESS.forward_checked(TASK_STACK_PAGE_COUNT).unwrap()
// };

// TODO
// let task = crate::local_state::Task::new(
//     u8::MIN,
//     // TODO account for memory base when passing entry offset
//     crate::local_state::EntryPoint::Address(
//         Address::<Virtual>::new(elf.get_entry_offset() as u64).unwrap(),
//     ),
//     stack_address.address(),
//     {
//         #[cfg(target_arch = "x86_64")]
//         {
//             (
//                 crate::arch::x64::registers::GeneralRegisters::empty(),
//                 crate::arch::x64::registers::SpecialRegisters::flags_with_user_segments(
//                     crate::arch::x64::registers::RFlags::INTERRUPT_FLAG,
//                 ),
//             )
//         }
//     },
//     #[cfg(target_arch = "x86_64")]
//     {
//         // TODO do not error here ?
//         driver_page_manager.read_vmem_register().unwrap()
//     },
// );

// crate::local_state::queue_task(task);
