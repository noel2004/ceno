use crate::{
    circuit_builder::{CircuitBuilder, ConstraintSystem},
    error::ZKVMError,
    instructions::Instruction,
    tables::TableCircuit,
    witness::{LkMultiplicity, RowMajorMatrix},
};
use ceno_emul::StepRecord;
use ff_ext::ExtensionField;
use itertools::Itertools;
use multilinear_extensions::{
    mle::DenseMultilinearExtension, virtual_poly_v2::ArcMultilinearExtension,
};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use sumcheck::structs::IOPProverMessage;

pub struct TowerProver;

#[derive(Clone, Serialize)]
pub struct TowerProofs<E: ExtensionField> {
    pub proofs: Vec<Vec<IOPProverMessage<E>>>,
    // specs -> layers -> evals
    pub prod_specs_eval: Vec<Vec<Vec<E>>>,
    // specs -> layers -> point
    #[serde(skip)] // verifier can derive points itself
    pub prod_specs_points: Vec<Vec<Point<E>>>,
    // specs -> layers -> evals
    pub logup_specs_eval: Vec<Vec<Vec<E>>>,
    // specs -> layers -> point
    #[serde(skip)] // verifier can derive points itself
    pub logup_specs_points: Vec<Vec<Point<E>>>,
}

pub struct TowerProverSpec<'a, E: ExtensionField> {
    pub witness: Vec<Vec<ArcMultilinearExtension<'a, E>>>,
}

pub type WitnessId = u16;
pub type ChallengeId = u16;

pub enum ROMType {
    U5 = 0, // 2^5 = 32
    U16,    // 2^16 = 65,536
    And,    // a ^ b where a, b are bytes
    Ltu,    // a <(usign) b where a, b are bytes
}

#[derive(Clone, Debug, Copy)]
pub enum RAMType {
    GlobalState,
    Register,
}

/// A point is a vector of num_var length
pub type Point<F> = Vec<F>;

/// A point and the evaluation of this point.
#[derive(Clone, Debug, PartialEq)]
pub struct PointAndEval<F> {
    pub point: Point<F>,
    pub eval: F,
}

impl<E: ExtensionField> Default for PointAndEval<E> {
    fn default() -> Self {
        Self {
            point: vec![],
            eval: E::ZERO,
        }
    }
}

impl<F: Clone> PointAndEval<F> {
    /// Construct a new pair of point and eval.
    /// Caller gives up ownership
    pub fn new(point: Point<F>, eval: F) -> Self {
        Self { point, eval }
    }

    /// Construct a new pair of point and eval.
    /// Performs deep copy.
    pub fn new_from_ref(point: &Point<F>, eval: &F) -> Self {
        Self {
            point: (*point).clone(),
            eval: eval.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProvingKey<E: ExtensionField> {
    pub fixed_traces: Option<Vec<DenseMultilinearExtension<E>>>,
    pub vk: VerifyingKey<E>,
}

impl<E: ExtensionField> ProvingKey<E> {
    pub fn get_cs(&self) -> &ConstraintSystem<E> {
        self.vk.get_cs()
    }
}

#[derive(Clone, Debug)]
pub struct VerifyingKey<E: ExtensionField> {
    pub(crate) cs: ConstraintSystem<E>,
}

impl<E: ExtensionField> VerifyingKey<E> {
    pub fn get_cs(&self) -> &ConstraintSystem<E> {
        &self.cs
    }
}

#[derive(Default, Clone)]
pub struct ZKVMConstraintSystem<E: ExtensionField> {
    pub(crate) circuit_css: BTreeMap<String, ConstraintSystem<E>>,
}

impl<E: ExtensionField> ZKVMConstraintSystem<E> {
    pub fn register_opcode_circuit<OC: Instruction<E>>(&mut self) -> OC::InstructionConfig {
        let mut cs = ConstraintSystem::new(|| format!("riscv_opcode/{}", OC::name()));
        let mut circuit_builder = CircuitBuilder::<E>::new(&mut cs);
        let config = OC::construct_circuit(&mut circuit_builder).unwrap();
        assert!(self.circuit_css.insert(OC::name(), cs).is_none());

        config
    }

    pub fn register_table_circuit<TC: TableCircuit<E>>(&mut self) -> TC::TableConfig {
        let mut cs = ConstraintSystem::new(|| format!("riscv_table/{}", TC::name()));
        let mut circuit_builder = CircuitBuilder::<E>::new(&mut cs);
        let config = TC::construct_circuit(&mut circuit_builder).unwrap();
        assert!(self.circuit_css.insert(TC::name(), cs.clone()).is_none());

        config
    }

    pub fn get_cs(&self, name: &String) -> Option<&ConstraintSystem<E>> {
        self.circuit_css.get(name)
    }
}

#[derive(Default)]
pub struct ZKVMFixedTraces<E: ExtensionField> {
    pub circuit_fixed_traces: BTreeMap<String, Option<RowMajorMatrix<E::BaseField>>>,
}

impl<E: ExtensionField> ZKVMFixedTraces<E> {
    pub fn register_opcode_circuit<OC: Instruction<E>>(&mut self, _cs: &ZKVMConstraintSystem<E>) {
        assert!(self.circuit_fixed_traces.insert(OC::name(), None).is_none());
    }

    pub fn register_table_circuit<TC: TableCircuit<E>>(
        &mut self,
        cs: &ZKVMConstraintSystem<E>,
        config: TC::TableConfig,
    ) {
        let cs = cs.get_cs(&TC::name()).expect("cs not found");
        assert!(
            self.circuit_fixed_traces
                .insert(
                    TC::name(),
                    Some(TC::generate_fixed_traces(&config, cs.num_fixed,)),
                )
                .is_none()
        );
    }
}

#[derive(Default)]
pub struct ZKVMWitnesses<E: ExtensionField> {
    pub witnesses: BTreeMap<String, RowMajorMatrix<E::BaseField>>,
    lk_mlts: BTreeMap<String, LkMultiplicity>,
    combined_lk_mlt: Option<Vec<HashMap<u64, usize>>>,
}

impl<E: ExtensionField> ZKVMWitnesses<E> {
    pub fn assign_opcode_circuit<OC: Instruction<E>>(
        &mut self,
        cs: &ZKVMConstraintSystem<E>,
        config: &OC::InstructionConfig,
        records: Vec<StepRecord>,
    ) -> Result<(), ZKVMError> {
        assert!(self.combined_lk_mlt.is_none());

        let cs = cs.get_cs(&OC::name()).unwrap();
        let (witness, logup_multiplicity) =
            OC::assign_instances(config, cs.num_witin as usize, records)?;
        assert!(self.witnesses.insert(OC::name(), witness).is_none());
        assert!(
            self.lk_mlts
                .insert(OC::name(), logup_multiplicity)
                .is_none()
        );

        Ok(())
    }

    // merge the multiplicities in each opcode circuit into one
    pub fn finalize_lk_multiplicities(&mut self) {
        assert!(self.combined_lk_mlt.is_none());
        assert!(!self.lk_mlts.is_empty());

        let mut combined_lk_mlt = vec![];
        let keys = self.lk_mlts.keys().cloned().collect_vec();
        for name in keys {
            let lk_mlt = self.lk_mlts.remove(&name).unwrap().into_finalize_result();
            if combined_lk_mlt.is_empty() {
                combined_lk_mlt = lk_mlt.to_vec();
            } else {
                combined_lk_mlt
                    .iter_mut()
                    .zip_eq(lk_mlt.iter())
                    .for_each(|(m1, m2)| {
                        for (key, value) in m2 {
                            *m1.entry(*key).or_insert(0) += value;
                        }
                    });
            }
        }

        self.combined_lk_mlt = Some(combined_lk_mlt);
    }

    pub fn assign_table_circuit<TC: TableCircuit<E>>(
        &mut self,
        cs: &ZKVMConstraintSystem<E>,
        config: &TC::TableConfig,
    ) -> Result<(), ZKVMError> {
        assert!(self.combined_lk_mlt.is_some());

        let cs = cs.get_cs(&TC::name()).unwrap();
        let witness = TC::assign_instances(
            config,
            cs.num_witin as usize,
            self.combined_lk_mlt.as_ref().unwrap(),
        )?;
        assert!(self.witnesses.insert(TC::name(), witness).is_none());

        Ok(())
    }
}

#[derive(Default)]
pub struct ZKVMProvingKey<E: ExtensionField> {
    // pk for opcode and table circuits
    pub(crate) circuit_pks: BTreeMap<String, ProvingKey<E>>,
}

impl<E: ExtensionField> ZKVMProvingKey<E> {
    pub fn get_vk(&self) -> ZKVMVerifyingKey<E> {
        ZKVMVerifyingKey {
            circuit_vks: self
                .circuit_pks
                .iter()
                .map(|(name, pk)| (name.clone(), pk.vk.clone()))
                .collect(),
        }
    }
}

#[derive(Default, Clone)]
pub struct ZKVMVerifyingKey<E: ExtensionField> {
    // pk for opcode and table circuits
    pub circuit_vks: BTreeMap<String, VerifyingKey<E>>,
}