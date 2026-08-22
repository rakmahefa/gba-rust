use std::collections::BTreeSet;

use crate::semantic_ir::SemanticProgram;

use super::common::{block_name, mode_bool};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedBlock {
    pub address: u32,
    pub thumb: bool,
    pub symbol: String,
}

impl LinkedBlock {
    pub fn key(&self) -> (u32, bool) {
        (self.address, self.thumb)
    }
}

pub fn collect_linked_blocks(semantic: &SemanticProgram) -> Vec<LinkedBlock> {
    let mut seen = BTreeSet::new();
    let mut blocks = Vec::new();

    for function in &semantic.functions {
        for block in &function.blocks {
            let key = (block.address, mode_bool(block.mode));
            assert!(
                seen.insert(key),
                "duplicate generated CFG block key: {:#010x}/{}",
                key.0,
                if key.1 { "Thumb" } else { "ARM" }
            );
            blocks.push(LinkedBlock {
                address: key.0,
                thumb: key.1,
                symbol: block_name(block.id, block.mode, block.address),
            });
        }
    }

    blocks.sort_unstable_by_key(LinkedBlock::key);
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{analyze, build_semantic_program, discover_functions, Mode, ROM_BASE};

    #[test]
    fn linked_block_set_is_canonicalized_by_address_and_mode() {
        let mut rom = Vec::new();
        rom.extend_from_slice(&0xe3a0_0001u32.to_le_bytes());
        rom.extend_from_slice(&0xe280_0001u32.to_le_bytes());
        let program = analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let blocks = collect_linked_blocks(&semantic);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].key(), (ROM_BASE, false));
        assert!(!blocks[0].symbol.is_empty());
    }
}
