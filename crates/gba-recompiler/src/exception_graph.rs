use std::collections::{HashMap, HashSet};

use crate::address_space::{ImageKind, ImageMapping};
use crate::cfg::{analyze_with_mapping, BlockKey, Program};
use crate::decoder::{ArmDataOp, ArmExtended, ArmOp, InstructionKind, Mode};

/// Architectural exception vectors that are part of the ARM7TDMI exception graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExceptionVectorKind {
    Undefined,
    SoftwareInterrupt,
    PrefetchAbort,
    DataAbort,
    Irq,
    Fiq,
}

impl ExceptionVectorKind {
    pub const ALL: [Self; 6] = [
        Self::Undefined,
        Self::SoftwareInterrupt,
        Self::PrefetchAbort,
        Self::DataAbort,
        Self::Irq,
        Self::Fiq,
    ];

    pub const fn vector(self) -> u32 {
        match self {
            Self::Undefined => 0x04,
            Self::SoftwareInterrupt => 0x08,
            Self::PrefetchAbort => 0x0c,
            Self::DataAbort => 0x10,
            Self::Irq => 0x18,
            Self::Fiq => 0x1c,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionGraphEdgeKind {
    SharedHandler,
    ExceptionReturn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionGraphEdge {
    pub from: ExceptionVectorKind,
    pub to: Option<ExceptionVectorKind>,
    pub kind: ExceptionGraphEdgeKind,
    pub address: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionGraphNode {
    pub kind: ExceptionVectorKind,
    pub vector: u32,
    pub program: Program,
    pub exception_return_sites: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionGraph {
    pub image: ImageMapping,
    pub nodes: Vec<ExceptionGraphNode>,
    pub edges: Vec<ExceptionGraphEdge>,
}

impl ExceptionGraph {
    pub fn node(&self, kind: ExceptionVectorKind) -> Option<&ExceptionGraphNode> {
        self.nodes.iter().find(|node| node.kind == kind)
    }

    pub fn vector_keys(&self) -> HashSet<BlockKey> {
        self.nodes
            .iter()
            .map(|node| BlockKey {
                address: node.vector,
                mode: Mode::Arm,
            })
            .collect()
    }

    pub fn shared_handler_edges(&self) -> impl Iterator<Item = &ExceptionGraphEdge> {
        self.edges
            .iter()
            .filter(|edge| edge.kind == ExceptionGraphEdgeKind::SharedHandler)
    }
}

fn exception_return_sites(program: &Program) -> Vec<u32> {
    program
        .cfg
        .blocks
        .iter()
        .flat_map(|block| block.instructions.iter())
        .filter_map(|instruction| {
            let InstructionKind::Arm(op) = instruction.kind else {
                return None;
            };
            match op {
                ArmOp::Extended(ArmExtended::DataProcessing {
                    op,
                    rd: 15,
                    set_flags: true,
                    ..
                }) if !matches!(
                    op,
                    ArmDataOp::Tst | ArmDataOp::Teq | ArmDataOp::Cmp | ArmDataOp::Cmn
                ) => Some(instruction.address),
                _ => None,
            }
        })
        .collect()
}

/// Analyze each architectural exception vector as its own CFG root and build
/// the shared-handler/exception-return relationships between those CFGs.
pub fn analyze_exception_graph(
    image: &[u8],
    mut mapping: ImageMapping,
) -> Result<ExceptionGraph, crate::cfg::AnalysisError> {
    if mapping.kind != ImageKind::Bios {
        mapping.kind = ImageKind::Bios;
    }

    let mut nodes = Vec::with_capacity(ExceptionVectorKind::ALL.len());
    for kind in ExceptionVectorKind::ALL {
        mapping.entry = kind.vector();
        mapping.entry_mode = Mode::Arm;
        let program = analyze_with_mapping(image, mapping)?;
        let exception_return_sites = exception_return_sites(&program);
        nodes.push(ExceptionGraphNode {
            kind,
            vector: kind.vector(),
            program,
            exception_return_sites,
        });
    }

    let mut block_owners = HashMap::<BlockKey, ExceptionVectorKind>::new();
    let mut edges = Vec::new();
    for node in &nodes {
        for block in &node.program.cfg.blocks {
            let key = block.key.clone();
            if let Some(owner) = block_owners.get(&key).copied() {
                if owner != node.kind {
                    edges.push(ExceptionGraphEdge {
                        from: node.kind,
                        to: Some(owner),
                        kind: ExceptionGraphEdgeKind::SharedHandler,
                        address: key.address,
                    });
                }
            } else {
                block_owners.insert(key, node.kind);
            }
        }
        for &address in &node.exception_return_sites {
            edges.push(ExceptionGraphEdge {
                from: node.kind,
                to: None,
                kind: ExceptionGraphEdgeKind::ExceptionReturn,
                address,
            });
        }
    }

    edges.sort_by_key(|edge| (edge.from as u8, edge.kind as u8, edge.address));
    edges.dedup();

    Ok(ExceptionGraph {
        image: mapping,
        nodes,
        edges,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address_space::{ImageKind, ImageMapping};

    fn arm_image(words: &[(u32, u32)]) -> Vec<u8> {
        let mut image = vec![0u8; 0x20];
        for &(address, raw) in words {
            image[address as usize..address as usize + 4].copy_from_slice(&raw.to_le_bytes());
        }
        image
    }

    #[test]
    fn analyzes_all_arm_exception_vectors_as_independent_roots() {
        let image = arm_image(&[
            (0x00, 0xe1a0_0000),
            (0x04, 0xe1a0_0000),
            (0x08, 0xe1a0_0000),
            (0x0c, 0xe1a0_0000),
            (0x10, 0xe1a0_0000),
            (0x14, 0xe1a0_0000),
            (0x18, 0xe1a0_0000),
            (0x1c, 0xe1a0_0000),
        ]);
        let mapping = ImageMapping::new(ImageKind::Bios, 0, image.len() as u32, 0, Mode::Arm);
        let graph = analyze_exception_graph(&image, mapping).unwrap();

        assert_eq!(graph.nodes.len(), 6);
        assert_eq!(graph.vector_keys().len(), ExceptionVectorKind::ALL.len());
        assert!(graph.node(ExceptionVectorKind::Irq).is_some());
        assert!(graph.node(ExceptionVectorKind::Fiq).is_some());
    }

    #[test]
    fn records_exception_return_sites_as_graph_edges() {
        let image = arm_image(&[(0x18, 0xe25e_f004)]);
        let mapping = ImageMapping::new(ImageKind::Bios, 0, image.len() as u32, 0, Mode::Arm);
        let graph = analyze_exception_graph(&image, mapping).unwrap();

        let irq = graph.node(ExceptionVectorKind::Irq).unwrap();
        assert_eq!(irq.exception_return_sites, vec![0x18]);
        assert!(graph.edges.iter().any(|edge| {
            edge.from == ExceptionVectorKind::Irq
                && edge.kind == ExceptionGraphEdgeKind::ExceptionReturn
                && edge.address == 0x18
        }));
    }
}
