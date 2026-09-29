//! Structural validation shared by insertion and archive loading.
use super::{
    AssemblyRevisionId, BehaviorBinding, BlueprintCatalog, BlueprintError, BlueprintPortKind,
    BlueprintRevisionId, ExpansionLimits, PositionedBlock, TypeContract, TypeRevisionId,
};
use crate::{LogicalSpec, PortDirection};
use std::collections::BTreeSet;

impl BlueprintCatalog {
    pub(super) fn validate_assembly_revision(
        &self,
        id: &AssemblyRevisionId,
    ) -> Result<(), BlueprintError> {
        let record = self
            .assembly(id)
            .ok_or_else(|| BlueprintError::UnknownAssembly(id.clone()))?;
        unique(record.parents.iter(), "duplicate assembly parent")?;
        let mut pending = record.parents.clone();
        let mut seen = BTreeSet::new();
        while let Some(parent) = pending.pop() {
            if &parent == id {
                return Err(BlueprintError::AssemblyAncestryCycle(id.clone()));
            }
            if seen.insert(parent.clone()) {
                let record = self
                    .assembly(&parent)
                    .ok_or(BlueprintError::UnknownAssembly(parent))?;
                pending.extend(record.parents.iter().cloned());
            }
        }
        record.assembly.inspect(self)?;
        Ok(())
    }

    pub(super) fn validate_revision(&self, id: &BlueprintRevisionId) -> Result<(), BlueprintError> {
        let revision = self
            .revision(id)
            .ok_or_else(|| BlueprintError::UnknownRevision(id.clone()))?;
        if revision.name.trim().is_empty() {
            return invalid("blueprint name is empty");
        }
        validate_blocks(&revision.blocks)?;
        if let Some(law) = &revision.law {
            law.compile().map_err(BlueprintError::Invalid)?;
        }
        unique(
            revision.required_laws.iter(),
            "duplicate physical law requirement",
        )?;
        for law in &revision.required_laws {
            let source = self
                .revision(law)
                .ok_or_else(|| BlueprintError::UnknownRevision(law.clone()))?;
            if source.law.is_none() {
                return invalid(
                    "a required physical law must reference an executable law revision",
                );
            }
        }
        if let Some(layout) = &revision.initial_layout {
            if !revision.blocks.is_empty() {
                return invalid("captured layout and compositional blocks are mutually exclusive");
            }
            validate_blocks(&layout.blocks)?;
            if layout.known_regions.len() > ExpansionLimits::default().maximum_occurrences {
                return Err(BlueprintError::ExpansionLimit("layout regions"));
            }
            for region in &layout.known_regions {
                if region.min.x > region.max.x
                    || region.min.y > region.max.y
                    || region.min.z > region.max.z
                {
                    return invalid("invalid layout region");
                }
            }
        }
        unique(revision.parents.iter(), "duplicate revision parent")?;
        unique(revision.classifications.iter(), "duplicate classification")?;
        unique(
            revision.inclusions.iter().map(|child| &child.instance),
            "duplicate instance",
        )?;
        unique(
            revision.ports.iter().map(|port| &port.name),
            "duplicate port",
        )?;
        for classification in &revision.classifications {
            if !self.classifications.contains_key(classification) {
                return Err(BlueprintError::UnknownClassification(
                    classification.clone(),
                ));
            }
        }
        for port in &revision.ports {
            if port.name.trim().is_empty() {
                return invalid("port name is empty");
            }
            if port.direction != PortDirection::Input && !port.required_source_types.is_empty() {
                return invalid("only a consumer input may require source types");
            }
            if port.kind == BlueprintPortKind::DeviceOutput
                && port.direction != PortDirection::Output
            {
                return invalid("device-output ports are output-only");
            }
            unique(
                port.required_source_types.iter(),
                "duplicate source requirement",
            )?;
            for required in &port.required_source_types {
                self.require_type(required)?;
            }
        }
        unique(
            revision.static_type_bindings.iter(),
            "duplicate static type binding",
        )?;
        for binding in &revision.static_type_bindings {
            let definition = self
                .type_revision(&binding.type_revision)
                .ok_or_else(|| BlueprintError::UnknownType(binding.type_revision.clone()))?;
            if !matches!(
                definition.contract,
                TypeContract::Signal { .. }
                    | TypeContract::BlockKind { .. }
                    | TypeContract::BlockPattern { .. }
            ) {
                return invalid(
                    "static type bindings require snapshot contracts, not behavioral relations",
                );
            }
            if !revision.ports.iter().any(|port| port.name == binding.port) {
                return invalid("static type binding requires a named physical port");
            }
        }
        unique(
            revision.behavior_bindings.iter(),
            "duplicate behavior binding",
        )?;
        for binding in &revision.behavior_bindings {
            let definition = self
                .type_revision(binding.behavior_type())
                .ok_or_else(|| BlueprintError::UnknownType(binding.behavior_type().clone()))?;
            let valid_port = |name: &str, direction| {
                revision.ports.iter().any(|port| {
                    port.name == name
                        && port.direction == direction
                        && port.kind != BlueprintPortKind::BlockState
                })
            };
            match (binding, &definition.contract) {
                (
                    BehaviorBinding::Observed {
                        observed_inputs,
                        observed_outputs,
                        ..
                    },
                    contract,
                ) => {
                    use crate::location_observation::ObservedPort;
                    let (inputs, outputs): (Vec<&str>, Vec<&str>) = match contract {
                        TypeContract::PistonDoor { requirement } => (
                            vec![requirement.closed_input.as_str()],
                            requirement
                                .aperture
                                .iter()
                                .flatten()
                                .flat_map(|cell| [cell.air.as_str(), cell.solid.as_str()])
                                .collect(),
                        ),
                        TypeContract::RepeatedSettling { relation } => (
                            relation.inputs.iter().map(String::as_str).collect(),
                            relation.outputs.iter().map(String::as_str).collect(),
                        ),
                        TypeContract::SingleOperation { requirement } => (
                            vec![requirement.input.as_str()],
                            requirement.outputs.iter().map(String::as_str).collect(),
                        ),
                        TypeContract::Periodic { requirement } => {
                            (vec![], vec![requirement.output.as_str()])
                        }
                        TypeContract::FiniteBurst { requirement } => {
                            (vec![], vec![requirement.output.as_str()])
                        }
                        _ => return invalid("observed binding requires a behavioral type"),
                    };
                    if observed_inputs
                        .keys()
                        .map(String::as_str)
                        .collect::<BTreeSet<_>>()
                        != inputs.into_iter().collect()
                        || observed_outputs
                            .keys()
                            .map(String::as_str)
                            .collect::<BTreeSet<_>>()
                            != outputs.into_iter().collect()
                    {
                        return invalid(
                            "observed binding must map every named type input and output exactly",
                        );
                    }
                    unique(
                        observed_inputs.values().map(|o| o.port()),
                        "independent behavior inputs require distinct physical ports",
                    )?;
                    for (observations, direction) in [
                        (observed_inputs, PortDirection::Input),
                        (observed_outputs, PortDirection::Output),
                    ] {
                        for observation in observations.values() {
                            let Some(port) = revision
                                .ports
                                .iter()
                                .find(|p| p.name == observation.port() && p.direction == direction)
                            else {
                                return invalid(
                                    "observed binding requires a named port with matching direction",
                                );
                            };
                            match observation {
                                ObservedPort::Signal { .. }
                                    if port.kind == BlueprintPortKind::BlockState =>
                                {
                                    return invalid(
                                        "signal observation cannot reinterpret a block-state port",
                                    );
                                }
                                ObservedPort::Location { predicate, .. } => {
                                    if port.kind != BlueprintPortKind::BlockState {
                                        return invalid(
                                            "location observation requires an explicit block-state port",
                                        );
                                    }
                                    predicate
                                        .validate()
                                        .map_err(|reason| BlueprintError::Invalid(reason.into()))?;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                (
                    BehaviorBinding::Autonomous { output_port, .. },
                    TypeContract::Periodic { .. } | TypeContract::FiniteBurst { .. },
                ) => {
                    if !valid_port(output_port, PortDirection::Output) {
                        return invalid("behavior binding requires a named signal output port");
                    }
                }
                (
                    BehaviorBinding::RepeatedSettling {
                        inputs, outputs, ..
                    },
                    TypeContract::RepeatedSettling { relation },
                ) => {
                    if inputs.keys().collect::<BTreeSet<_>>()
                        != relation.inputs.iter().collect::<BTreeSet<_>>()
                        || outputs.keys().collect::<BTreeSet<_>>()
                            != relation.outputs.iter().collect::<BTreeSet<_>>()
                    {
                        return invalid(
                            "behavior binding must map every named type input and output exactly",
                        );
                    }
                    if inputs
                        .values()
                        .any(|name| !valid_port(name, PortDirection::Input))
                        || outputs
                            .values()
                            .any(|name| !valid_port(name, PortDirection::Output))
                    {
                        return invalid(
                            "behavior binding requires named signal ports with matching directions",
                        );
                    }
                    unique(
                        inputs.values(),
                        "independent behavior inputs require distinct physical ports",
                    )?;
                }
                _ => return invalid("behavior binding shape does not match its type contract"),
            }
        }
        // Ancestry and containment may share references, but their cycles mean
        // different things and are checked independently.
        let mut pending = revision.parents.clone();
        let mut seen = BTreeSet::new();
        while let Some(parent) = pending.pop() {
            if &parent == id {
                return Err(BlueprintError::AncestryCycle(id.clone()));
            }
            if seen.insert(parent.clone()) {
                let record = self
                    .revision(&parent)
                    .ok_or_else(|| BlueprintError::UnknownRevision(parent.clone()))?;
                pending.extend(record.parents.iter().cloned());
            }
        }
        let occurrences = self
            .expand(id)?
            .occurrences
            .into_iter()
            .map(|occurrence| (occurrence.path.clone(), occurrence))
            .collect();
        crate::interfaces::InterfaceIndex::new(self, &occurrences)?;
        Ok(())
    }

    pub(super) fn require_type(&self, id: &TypeRevisionId) -> Result<(), BlueprintError> {
        self.types
            .contains_key(id)
            .then_some(())
            .ok_or_else(|| BlueprintError::UnknownType(id.clone()))
    }
}

pub(crate) fn validate_blocks(blocks: &[PositionedBlock]) -> Result<(), BlueprintError> {
    unique(
        blocks.iter().map(|block| block.position),
        "duplicate local block position",
    )
}

pub(crate) fn unique<T: Ord>(
    values: impl IntoIterator<Item = T>,
    message: &str,
) -> Result<(), BlueprintError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return invalid(message);
        }
    }
    Ok(())
}

pub(super) fn validate_function(spec: &LogicalSpec) -> Result<(), BlueprintError> {
    if spec.stateful {
        return invalid("BooleanFunction does not describe stateful behavior");
    }
    if spec
        .ports
        .iter()
        .any(|port| port.name.trim().is_empty() || port.bit_width != 1)
    {
        return invalid("BooleanFunction requires named single-bit ports");
    }
    unique(
        spec.ports.iter().map(|port| &port.name),
        "duplicate function port",
    )?;
    let inputs = spec.input_names().len();
    if inputs == 0 || spec.output_names().is_empty() || inputs >= usize::BITS as usize {
        return invalid("unsupported BooleanFunction arity");
    }
    if spec.truth_table.len() != 1_usize << inputs {
        return invalid("BooleanFunction needs every input assignment");
    }
    let mut assignments = BTreeSet::new();
    for row in &spec.truth_table {
        if row.len() != spec.ports.len() || !assignments.insert(&row[..inputs]) {
            return invalid("invalid or repeated BooleanFunction row");
        }
    }
    Ok(())
}

pub(crate) fn invalid<T>(message: &str) -> Result<T, BlueprintError> {
    Err(BlueprintError::Invalid(message.to_owned()))
}
