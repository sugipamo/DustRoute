//! Resolve explicit location bindings and sample the actual moving world.
//! Samples are diagnostics, not behavioral certificates or adoption authority.
use std::collections::BTreeMap;

use dustroute_library::assembly::{Assembly, AssemblyPortRef};
use dustroute_library::blueprint::{
    BehaviorBinding, BlueprintCatalog, BlueprintPortKind, BlueprintRevisionId, InstancePath,
    TypeRevisionId,
};
use dustroute_library::location_observation::{LocationPredicate, ObservedPort};
use dustroute_minecraft::Pos;
use dustroute_minecraft::time::piston_runtime::ElectricalPistonRuntime;
use dustroute_minecraft::time::runtime::LocationObservation;
use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct FixedObservation {
    pub(crate) position: Pos,
    pub(crate) predicate: LocationPredicate,
}

/// Positions are resolved once from the concrete Assembly. Neither sampling
/// nor physical movement can replace the source, its children, or these ports.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LocationBehaviorBinding {
    source: BlueprintRevisionId,
    instance: InstancePath,
    behavior_type: TypeRevisionId,
    pub(crate) inputs: BTreeMap<String, FixedObservation>,
    pub(crate) outputs: BTreeMap<String, FixedObservation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ObservedBoolean {
    pub state: LocationObservation,
    pub value: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LocationBehaviorSample {
    pub behavior_type: TypeRevisionId,
    pub execution_profile: &'static str,
    pub inputs: BTreeMap<String, ObservedBoolean>,
    pub outputs: BTreeMap<String, ObservedBoolean>,
}

impl LocationBehaviorBinding {
    pub(crate) fn validate_door(
        &self,
        requirement: &dustroute_library::behavior_type::PistonDoor,
    ) -> Result<(), String> {
        use dustroute_minecraft::BlockKind;
        let mut cells = Vec::new();
        for cell in requirement.aperture.iter().flatten() {
            let air = self
                .outputs
                .get(&cell.air)
                .ok_or("missing aperture Air observation")?;
            let solid = self
                .outputs
                .get(&cell.solid)
                .ok_or("missing aperture Solid observation")?;
            if air.position != solid.position
                || air.predicate
                    != (LocationPredicate::BlockKind {
                        block_kind: BlockKind::Air,
                    })
                || solid.predicate
                    != (LocationPredicate::BlockKind {
                        block_kind: BlockKind::Solid,
                    })
            {
                return Err(
                    "each door cell requires Air and Solid observations of the same fixed location"
                        .into(),
                );
            }
            cells.push([
                i64::from(air.position.x),
                i64::from(air.position.y),
                i64::from(air.position.z),
            ]);
        }
        let origin = cells[0];
        let row: [i64; 3] = std::array::from_fn(|axis| cells[3][axis] - origin[axis]);
        let column: [i64; 3] = std::array::from_fn(|axis| cells[1][axis] - origin[axis]);
        if row.iter().map(|v| v.abs()).sum::<i64>() != 1
            || column.iter().map(|v| v.abs()).sum::<i64>() != 1
            || row.iter().zip(column).map(|(a, b)| a * b).sum::<i64>() != 0
        {
            return Err("door aperture needs perpendicular unit row/column axes".into());
        }
        for (index, cell) in cells.iter().enumerate() {
            let expected: [i64; 3] = std::array::from_fn(|axis| {
                origin[axis] + row[axis] * (index / 3) as i64 + column[axis] * (index % 3) as i64
            });
            if *cell != expected {
                return Err("door observations must form a complete fixed 3x3 plane".into());
            }
        }
        Ok(())
    }

    /// This location-only sampler is intentionally separate from electrical
    /// sampling: a signal selection must use its world's electrical adapter.
    /// Catalog validation is structural and does not constitute a behavior pass.
    pub fn resolve(
        catalog: &BlueprintCatalog,
        assembly: &Assembly,
        instance: &InstancePath,
        binding: &BehaviorBinding,
    ) -> Result<Self, String> {
        let view = assembly.inspect(catalog).map_err(|e| e.to_string())?;
        let occurrence = view
            .occurrences
            .get(instance)
            .ok_or("unknown binding occurrence")?;
        let source = catalog
            .revision(&occurrence.revision)
            .ok_or("unknown binding source")?;
        if !source.behavior_bindings.contains(binding) {
            return Err("the selected source does not declare this binding".into());
        }
        let BehaviorBinding::Observed {
            behavior_type,
            observed_inputs,
            observed_outputs,
        } = binding
        else {
            return Err("location sampling requires an explicit observed binding".into());
        };
        let resolve=|ports:&BTreeMap<String,ObservedPort>|->Result<BTreeMap<String,FixedObservation>,String> {
            ports.iter().map(|(name,observation)| {
                let ObservedPort::Location {port,predicate}=observation else {return Err("signal observation requires an electrical sampling adapter".into());};
                let (terminal,_)=view.resolved_port(&AssemblyPortRef {instance:instance.clone(),port:port.clone()}).map_err(|e|e.to_string())?;
                if terminal.kind!=BlueprintPortKind::BlockState {return Err("location observation requires a block-state terminal".into());}
                predicate.validate().map_err(str::to_owned)?;
                Ok((name.clone(),FixedObservation {position:terminal.position,predicate:predicate.clone()}))
            }).collect()
        };
        Ok(Self {
            source: source.id.clone(),
            instance: instance.clone(),
            behavior_type: behavior_type.clone(),
            inputs: resolve(observed_inputs)?,
            outputs: resolve(observed_outputs)?,
        })
    }

    pub fn sample(
        &self,
        runtime: &ElectricalPistonRuntime,
    ) -> Result<LocationBehaviorSample, String> {
        let sample=|ports:&BTreeMap<String,FixedObservation>|->Result<BTreeMap<String,ObservedBoolean>,String> {
            ports.iter().map(|(name,observation)| {
                let state=runtime.view().observe_location(observation.position).map_err(|e|e.to_string())?;
                let value=observation.predicate.evaluate(&state)?;
                Ok((name.clone(),ObservedBoolean {state,value}))
            }).collect()
        };
        Ok(LocationBehaviorSample {
            behavior_type: self.behavior_type.clone(),
            execution_profile: dustroute_minecraft::time::piston_runtime::ELECTRICAL_PROFILE,
            inputs: sample(&self.inputs)?,
            outputs: sample(&self.outputs)?,
        })
    }
}
