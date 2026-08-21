use std::collections::{HashMap, HashSet};

use crate::cfg::Program;
use crate::decoder::Mode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LinkedBlockKey {
    pub address: u32,
    pub mode: Mode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedBlock {
    pub key: LinkedBlockKey,
    pub symbol: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    DuplicateBlock(LinkedBlockKey),
    MissingTarget(LinkedBlockKey),
}

#[derive(Debug, Clone, Default)]
pub struct GeneratedBlockLinker {
    blocks: HashMap<LinkedBlockKey, LinkedBlock>,
    linked_edges: HashSet<(LinkedBlockKey, LinkedBlockKey)>,
}

impl GeneratedBlockLinker {
    pub fn insert(&mut self, block: LinkedBlock) -> Result<(), LinkError> {
        if self.blocks.contains_key(&block.key) {
            return Err(LinkError::DuplicateBlock(block.key));
        }
        self.blocks.insert(block.key, block);
        Ok(())
    }

    pub fn from_program(program: &Program) -> Result<Self, LinkError> {
        let mut linker = Self::default();
        for block in &program.cfg.blocks {
            linker.insert(LinkedBlock {
                key: LinkedBlockKey {
                    address: block.key.address,
                    mode: block.key.mode,
                },
                symbol: generated_block_symbol(block.id.0 as u32, block.key.address, block.key.mode),
            })?;
        }
        for block in &program.cfg.blocks {
            let source = LinkedBlockKey {
                address: block.key.address,
                mode: block.key.mode,
            };
            for successor in &block.successors {
                let target_block = program
                    .cfg
                    .blocks
                    .get(successor.0)
                    .expect("CFG successor must reference a valid block");
                let target = LinkedBlockKey {
                    address: target_block.key.address,
                    mode: target_block.key.mode,
                };
                linker.link(source, target)?;
            }
        }
        Ok(linker)
    }

    pub fn link(&mut self, source: LinkedBlockKey, target: LinkedBlockKey) -> Result<&str, LinkError> {
        if !self.blocks.contains_key(&source) || !self.blocks.contains_key(&target) {
            return Err(LinkError::MissingTarget(if self.blocks.contains_key(&source) {
                target
            } else {
                source
            }));
        }
        self.linked_edges.insert((source, target));
        Ok(self
            .blocks
            .get(&target)
            .expect("target was validated above")
            .symbol
            .as_str())
    }

    pub fn resolve(&self, key: LinkedBlockKey) -> Option<&LinkedBlock> {
        self.blocks.get(&key)
    }

    pub fn is_linked(&self, key: LinkedBlockKey) -> bool {
        self.blocks.contains_key(&key)
    }

    pub fn linked_edge_count(&self) -> usize {
        self.linked_edges.len()
    }

    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    pub fn symbol_for(&self, key: LinkedBlockKey) -> Option<&str> {
        self.blocks.get(&key).map(|block| block.symbol.as_str())
    }
}

pub fn generated_block_symbol(id: u32, address: u32, mode: Mode) -> String {
    format!(
        "block_{id}_{}_{address:08x}",
        if matches!(mode, Mode::Thumb) { "thumb" } else { "arm" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{analyze, Mode, ROM_BASE};

    #[test]
    fn links_only_known_targets() {
        let source = LinkedBlockKey {
            address: 0x0800_0000,
            mode: Mode::Arm,
        };
        let target = LinkedBlockKey {
            address: 0x0800_0004,
            mode: Mode::Arm,
        };
        let mut linker = GeneratedBlockLinker::default();
        linker
            .insert(LinkedBlock {
                key: source,
                symbol: generated_block_symbol(0, source.address, source.mode),
            })
            .unwrap();
        linker
            .insert(LinkedBlock {
                key: target,
                symbol: generated_block_symbol(1, target.address, target.mode),
            })
            .unwrap();

        assert_eq!(linker.link(source, target).unwrap(), "block_1_arm_08000004");
        assert_eq!(linker.linked_edge_count(), 1);
        assert!(linker.is_linked(target));
    }

    #[test]
    fn builds_links_from_cfg_successors() {
        let words = [0xEA00_0000u32, 0xE1A0_0000u32];
        let mut rom = Vec::new();
        for word in words {
            rom.extend_from_slice(&word.to_le_bytes());
        }
        let program = analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let linker = GeneratedBlockLinker::from_program(&program).unwrap();
        assert_eq!(linker.block_count(), program.cfg.blocks.len());
        assert!(linker.linked_edge_count() >= 1);
    }

    #[test]
    fn rejects_duplicate_blocks() {
        let key = LinkedBlockKey {
            address: 0x0800_0000,
            mode: Mode::Arm,
        };
        let block = LinkedBlock {
            key,
            symbol: generated_block_symbol(0, key.address, key.mode),
        };
        let mut linker = GeneratedBlockLinker::default();
        linker.insert(block.clone()).unwrap();
        assert_eq!(linker.insert(block), Err(LinkError::DuplicateBlock(key)));
    }
}
