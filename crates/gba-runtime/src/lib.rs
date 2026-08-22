mod arm7tdmi;
mod bios;
mod bios_memory;
mod contract;
mod dma;
mod execution;

pub mod apu;
pub mod bus;
pub mod cartridge;
pub mod cpu;
pub mod mmio;
pub mod mmio_devices;
pub mod ppu;
pub mod runtime;
pub mod scheduler;
pub mod timers;

pub use apu::Apu;
pub use bios::{
    execute_swi, service_pending_irq, BiosMemory, BiosResult, BiosSwi, InterruptController,
    PowerState, BIOS_END, BIOS_START, DISPSTAT, EWRAM_END, EWRAM_START, HALTCNT, IE, IF, IME,
    IO_END, IO_START, IRQ_DMA0, IRQ_DMA1, IRQ_DMA2, IRQ_DMA3, IRQ_GAMEPAK, IRQ_HBLANK, IRQ_KEYPAD,
    IRQ_SERIAL, IRQ_TIMER0, IRQ_TIMER1, IRQ_TIMER2, IRQ_TIMER3, IRQ_VBLANK, IRQ_VCOUNT, IWRAM_END,
    IWRAM_START, KEYINPUT, OAM_END, OAM_START, PALETTE_END, PALETTE_START, POSTFLG, VRAM_END,
    VRAM_START, WAITCNT,
};
pub use bios_memory::{Bios, BiosLoadError, BIOS_SIZE};
pub use bus::{decode as decode_bus_address, BusAddress, BusRegion};
pub use cartridge::{detect_save_type, Cartridge, SaveRam, SaveType};
pub use cpu::{
    BankedRegisters, Cpu, CpuMode, ExceptionKind, CPSR_C, CPSR_N, CPSR_V, CPSR_Z, REG_LR, REG_PC,
    REG_SP,
};
pub use dma::{DmaAddressMode, DmaChannel, DmaController, DmaTrigger, DmaTransfer};
pub use mmio::{DISPSTAT_HBLANK, DISPSTAT_VBLANK, DISPSTAT_VCOUNT_IRQ, VCOUNT};
pub use ppu::Ppu;
pub use runtime::Runtime;
pub use scheduler::{
    EventKind, ScheduledEvent, TimingScheduler, CYCLES_PER_SCANLINE, HBLANK_START_CYCLES,
    SCANLINES_PER_FRAME, VBLANK_START_LINE,
};
pub use timers::{Timer, TimerControl, TimerState, TIMER_COUNT};

pub use arm7tdmi::Nzcv;
pub use arm7tdmi::{
    condition_holds as architectural_condition_holds, exchange_target, link_address,
    shift_immediate, shift_register, ShiftKind, ShiftResult,
};
pub use contract::{
    ArchitecturalState, GeneratedBlockExit, GeneratedBlockKey, GeneratedExecutionExit,
    GeneratedExecutionResult, RuntimeContract, GENERATED_BIOS_SWI_UNIMPLEMENTED,
    GENERATED_TARGET_DYNAMIC_UNRESOLVED, GENERATED_TARGET_MISALIGNED, GENERATED_TARGET_OUTSIDE_CFG,
    RUNTIME_CONTRACT_VERSION,
};
pub use timers::{
    TIMER0CNT_H, TIMER0CNT_L, TIMER1CNT_H, TIMER1CNT_L, TIMER2CNT_H, TIMER2CNT_L, TIMER3CNT_H,
    TIMER3CNT_L,
};

pub const WIDTH: usize = 240;
pub const HEIGHT: usize = 160;
