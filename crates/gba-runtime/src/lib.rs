mod arm7tdmi;
mod bios;
mod bios_memory;
mod cartridge;
mod contract;
mod cpu;
mod execution;
mod ppu;
mod runtime;

pub use bios::{BiosMemory, BiosResult, BiosSwi, InterruptController, PowerState};
pub use bios_memory::{Bios, BiosLoadError, BIOS_SIZE};
pub use cartridge::{Cartridge, SaveDevice, SaveType};
pub use contract::{
    ArchitecturalState, GeneratedBlockExit, GeneratedBlockKey, GeneratedExecutionExit,
    GeneratedExecutionResult, RuntimeContract, RUNTIME_CONTRACT_VERSION,
};
pub use cpu::{Cpu, CpuMode};
pub use ppu::Ppu;
pub use runtime::Runtime;

pub const REG_PC: usize = 15;
pub const REG_LR: usize = 14;
pub const REG_SP: usize = 13;
