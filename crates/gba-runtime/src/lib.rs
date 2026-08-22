mod arm7tdmi;
mod contract;
mod execution;

pub mod apu;
pub mod cartridge;
pub mod cpu;
pub mod ppu;
pub mod runtime;

pub use apu::Apu;
pub use cartridge::{detect_save_type, Cartridge, SaveRam, SaveType};
pub use cpu::{
    BankedRegisters, Cpu, CpuMode, ExceptionKind, CPSR_C, CPSR_N, CPSR_V, CPSR_Z, REG_LR,
    REG_PC, REG_SP,
};
pub use ppu::Ppu;
pub use runtime::Runtime;

pub use arm7tdmi::Nzcv;
pub use arm7tdmi::{
    condition_holds as architectural_condition_holds, exchange_target, link_address,
    shift_immediate, shift_register, ShiftKind, ShiftResult,
};
pub use contract::{
    ArchitecturalState, GeneratedBlockExit, GeneratedBlockKey, GeneratedExecutionExit,
    GeneratedExecutionResult, RuntimeContract, GENERATED_TARGET_MISALIGNED,
    GENERATED_TARGET_OUTSIDE_CFG, RUNTIME_CONTRACT_VERSION,
};

pub const WIDTH: usize = 240;
pub const HEIGHT: usize = 160;
