use std::collections::{HashMap, HashSet};

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

    pub fn link(&mut self, source: LinkedBlockKey, target: LinkedBlockKey) -> Result<&str, LinkError> {
        if !self.blocks.contains_key(&source) || !self.blocks.contains_key(&target) {
            return Err(LinkError::MissingTarget(if self.blocks.contains_key(&source) { target } else { source }));
        }
        self.linked_edges.insert((source, target));
        Ok(self.blocks.get(&target).expect("target was validated above").symbol.as_str())
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
