mod beq_circuit;
mod blt;
mod bltu;

use super::RIVInstruction;
use beq_circuit::BeqCircuit;
use ceno_emul::InsnKind;

#[cfg(test)]
mod test;

pub struct BeqOp;
impl RIVInstruction for BeqOp {
    const INST_KIND: InsnKind = InsnKind::BEQ;
}
pub type BeqInstruction<E> = BeqCircuit<E, BeqOp>;

pub struct BneOp;
impl RIVInstruction for BneOp {
    const INST_KIND: InsnKind = InsnKind::BNE;
}
pub type BneInstruction<E> = BeqCircuit<E, BneOp>;

pub struct BltuOp;
impl RIVInstruction for BltuOp {
    const INST_KIND: InsnKind = InsnKind::BLTU;
}
pub type BltuInstruction<E> = bltu::BltuCircuit<E, BltuOp>;

pub struct BgeuOp;
impl RIVInstruction for BgeuOp {
    const INST_KIND: InsnKind = InsnKind::BGEU;
}
pub type BgeuInstruction<E> = bltu::BltuCircuit<E, BgeuOp>;

pub struct BltOp;
impl RIVInstruction for BltOp {
    const INST_KIND: InsnKind = InsnKind::BLT;
}
pub type BltInstruction<E> = blt::BltCircuit<E, BltOp>;

pub struct BgeOp;
impl RIVInstruction for BgeOp {
    const INST_KIND: InsnKind = InsnKind::BGE;
}
pub type BgeInstruction<E> = blt::BltCircuit<E, BgeOp>;
