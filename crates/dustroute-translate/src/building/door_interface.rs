//! Resolve original typed terminals and align their physical occurrence frame.
use crate::assembly_transform::AssemblyTransform;
use dustroute_library::PortDirection;
use dustroute_library::assembly::{AssemblyBoundary, AssemblyPortRef};
use dustroute_library::blueprint::*;
use dustroute_library::location_observation::{LocationPredicate, ObservedPort};
use dustroute_minecraft::{BlockKind, Pos, RotationY};

pub(super) fn door_boundaries(
    instance: &InstancePath,
    binding: &BehaviorBinding,
    requirement: &dustroute_library::behavior_type::PistonDoor,
    view: &dustroute_library::assembly::AssemblyView,
) -> Result<(Vec<AssemblyBoundary>, bool), String> {
    let BehaviorBinding::Observed {
        observed_inputs,
        observed_outputs,
        ..
    } = binding
    else {
        return Err("door requires location observations".into());
    };
    let control = observed_inputs
        .get(&requirement.closed_input)
        .ok_or("missing closed-command observation")?;
    let ObservedPort::Location {
        predicate:
            LocationPredicate::Powered {
                block_kind: BlockKind::Lever,
                powered,
            },
        ..
    } = control
    else {
        return Err("door control must observe a lever with explicit polarity".into());
    };
    let mut ports = vec![("door_control".to_owned(), control, PortDirection::Input)];
    for (i, cell) in requirement.aperture.iter().flatten().enumerate() {
        ports.push((
            format!("door_aperture_{}_{}", i / 3, i % 3),
            &observed_outputs[&cell.air],
            PortDirection::Output,
        ));
    }
    let boundaries = ports
        .into_iter()
        .map(|(name, observation, direction)| {
            let ObservedPort::Location { port, .. } = observation else {
                return Err("door terminal must be a location".into());
            };
            let reference = AssemblyPortRef {
                instance: instance.clone(),
                port: port.clone(),
            };
            let (terminal, _) = view.resolved_port(&reference).map_err(|e| e.to_string())?;
            if terminal.direction != direction || terminal.kind != BlueprintPortKind::BlockState {
                return Err("door terminal has the wrong direction or interface kind".into());
            }
            Ok(AssemblyBoundary {
                name,
                port: reference,
            })
        })
        .collect::<Result<_, String>>()?;
    Ok((boundaries, *powered))
}

pub(super) fn align_aperture(
    aperture: &[Pos],
    rotation: RotationY,
    offset: i32,
) -> Result<AssemblyTransform, String> {
    let cells = aperture
        .iter()
        .map(|p| {
            rotation
                .checked_pos(*p)
                .ok_or("door rotation overflows coordinates".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let min = Pos::new(
        cells.iter().map(|p| p.x).min().unwrap(),
        cells.iter().map(|p| p.y).min().unwrap(),
        cells[0].z,
    );
    if cells.iter().any(|p| p.z != min.z)
        || cells.iter().map(|p| p.x).max().unwrap().checked_sub(min.x) != Some(2)
        || cells.iter().map(|p| p.y).max().unwrap().checked_sub(min.y) != Some(2)
    {
        return Err(
            "rotated door aperture must be a vertical 3x3 plane parallel to the north wall".into(),
        );
    }
    Ok(AssemblyTransform {
        source_anchor: Pos::default(),
        rotation,
        target_anchor: Pos::new(
            offset.checked_sub(min.x).ok_or("door alignment overflow")?,
            1_i32.checked_sub(min.y).ok_or("door alignment overflow")?,
            min.z.checked_neg().ok_or("door alignment overflow")?,
        ),
    })
}
