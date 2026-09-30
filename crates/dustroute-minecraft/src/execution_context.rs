//! World-level execution assumptions, independent of Blueprint interpretations.
//! This record selects an existing execution contract, not a live-world proof
//! or a snapshot of its mutable histories and pending work.
use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::PistonMotionProfile;
use crate::time::SchedulerProfile;

pub const TORCH_LAW_REVISION: &str = "dustroute.law.torch.java-1-21-11.v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorldExecutionProfile {
    #[serde(rename = "dustroute.dust-torch-synchronous-game-tick.v1")]
    DustTorchSynchronousGameTickV1,
    #[serde(rename = "dustroute.dust-single-torch-block-effects.v1")]
    DustSingleTorchBlockEffectsV1,
    #[serde(rename = "dustroute.redstone-compatibility-boundary.v1")]
    RedstoneCompatibilityBoundaryV1,
    #[serde(rename = "dustroute.bounded-redstone-events.v1")]
    BoundedRedstoneEventsV1,
    #[serde(rename = "dustroute.piston-electrical-callbacks.java-1-21-11.v19")]
    UnifiedPistonElectricalCallbacksJava12111V19,
}

impl WorldExecutionProfile {
    /// Adhesive motion and passive destruction require the current runtime.
    /// Recognizing a passive material must not widen an older executor.
    pub fn admits_block(self, block: &crate::Block) -> bool {
        self.admits_kind(block.kind)
            && (matches!(self, Self::UnifiedPistonElectricalCallbacksJava12111V19)
                || (!block.observed_name.as_deref().is_some_and(|name| {
                    crate::physical::plants::named(name).is_some()
                        || crate::physical::environment::named(name).is_some()
                }) && !block
                    .observed_name
                    .as_deref()
                    .and_then(crate::physical::passive::named)
                    .is_some_and(|s| {
                        s.adhesion() != crate::physical::Adhesion::None || s.piston_destroys()
                    })))
    }

    /// Kind admission is independent of registry membership and physical
    /// geometry. State, observed identity and known-space gates still apply.
    pub const fn admits_kind(self, kind: crate::BlockKind) -> bool {
        use crate::BlockKind::*;
        match self {
            Self::DustTorchSynchronousGameTickV1 | Self::DustSingleTorchBlockEffectsV1 => {
                matches!(
                    kind,
                    Air | Solid
                        | Transparent
                        | RedstoneWire
                        | RedstoneTorch
                        | Lever
                        | RedstoneBlock
                )
            }
            Self::RedstoneCompatibilityBoundaryV1 | Self::BoundedRedstoneEventsV1 => {
                crate::spatial::spatial_kind_v1(kind).is_some()
            }
            Self::UnifiedPistonElectricalCallbacksJava12111V19 => {
                matches!(
                    kind,
                    Air | Solid
                        | Transparent
                        | RedstoneWire
                        | Repeater
                        | RedstoneTorch
                        | Lever
                        | Button
                        | RedstoneLamp
                        | Comparator
                        | CopperBulb
                        | RedstoneBlock
                        | Observer
                        | Piston
                        | PistonHead
                        | MovingPiston
                )
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LawRole {
    BlockTraits,
    WireShape,
    WireTransfer,
    WireWeakPower,
    DustStrength,
    Torch,
    Repeater,
    Comparator,
    ComparatorSignal,
    Observer,
    Lamp,
    CopperBulb,
    Button,
    PistonState,
    PistonMotion,
    PistonConnection,
    SignalEmission,
    ConductorPower,
    PistonPayload,
    PistonControl,
    PistonCarrier,
    PistonGeometry,
    PistonHead,
}

/// The role assignment is explicit; registry array order is not a contract.
const DEVICE_LAWS: [(LawRole, &str); crate::device_program::DEVICE_COUNT] = {
    use crate::BlockKind;
    use crate::device_callback_law::law_id_for;
    let bindings = [
        (LawRole::Lamp, law_id_for(BlockKind::RedstoneLamp)),
        (LawRole::Observer, law_id_for(BlockKind::Observer)),
        (LawRole::Button, law_id_for(BlockKind::Button)),
        (LawRole::Repeater, law_id_for(BlockKind::Repeater)),
        (LawRole::CopperBulb, law_id_for(BlockKind::CopperBulb)),
        (LawRole::Comparator, law_id_for(BlockKind::Comparator)),
        (LawRole::Torch, law_id_for(BlockKind::RedstoneTorch)),
    ];
    let mut i = 0;
    while i < bindings.len() {
        let mut j = i + 1;
        while j < bindings.len() {
            assert!(
                bindings[i].0 as usize != bindings[j].0 as usize,
                "duplicate device law role"
            );
            j += 1;
        }
        i += 1;
    }
    bindings
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InitializationPolicy {
    /// Declared torch lit state, empty histories/timers, then initial notification.
    FreshTorchConstruction,
    /// Existing device-cache seeds and queues; no claim of recovered live history.
    CompatibilityDeviceSeeds,
    /// Supplied blocks and an empty queue. Resuming work requires a checkpoint.
    SuppliedBlocksEmptyQueue,
    FreshElectricalPistonConstruction,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputPolicy {
    PhysicalLeversBetweenModelSteps,
    ExplicitCompatibilityMutations,
    ExplicitScheduledWorldEvents,
    PhysicalDeviceUsesBetweenSynchronousCalls,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorldExecutionContext {
    pub profile: WorldExecutionProfile,
    pub laws: BTreeMap<LawRole, String>,
    pub initialization: InitializationPolicy,
    pub inputs: InputPolicy,
    pub scheduler: Option<SchedulerProfile>,
    pub max_electrical_iterations: Option<usize>,
    pub piston_motion: Option<PistonMotionProfile>,
}

impl WorldExecutionContext {
    pub fn validate_world_kinds(&self, world: &crate::World) -> Result<(), String> {
        self.validate()?;
        for (pos, block) in world.iter() {
            if !self.profile.admits_block(block) {
                return Err(format!(
                    "execution profile {:?} does not admit {:?} at {pos:?}",
                    self.profile, block.kind
                ));
            }
        }
        Ok(())
    }

    /// Compiled physical declarations are an immutable adapter dependency,
    /// separate from the legacy finite spatial-law catalog.
    pub const fn physical_admission_revision(&self) -> Option<&'static str> {
        match self.profile {
            WorldExecutionProfile::UnifiedPistonElectricalCallbacksJava12111V19 => {
                Some(crate::physical::REVISION)
            }
            _ => None,
        }
    }
    pub fn for_profile(profile: WorldExecutionProfile) -> Self {
        use LawRole::*;
        use WorldExecutionProfile::*;
        let mut laws: BTreeMap<_, _> = [BlockTraits, WireShape, WireTransfer, WireWeakPower]
            .into_iter()
            .zip(crate::spatial::SPATIAL_LAW_IDS)
            .map(|(role, id)| (role, id.to_owned()))
            .collect();
        laws.insert(DustStrength, crate::dust_law::DUST_LAW_REVISION.into());
        let (initialization, inputs, scheduler, max_electrical_iterations, piston_motion) =
            match profile {
                DustTorchSynchronousGameTickV1 | DustSingleTorchBlockEffectsV1 => {
                    laws.insert(Torch, TORCH_LAW_REVISION.into());
                    (
                        InitializationPolicy::FreshTorchConstruction,
                        InputPolicy::PhysicalLeversBetweenModelSteps,
                        None,
                        Some(128),
                        None,
                    )
                }
                RedstoneCompatibilityBoundaryV1 => {
                    laws.extend(
                        [
                            (Torch, TORCH_LAW_REVISION),
                            (Repeater, crate::repeater_law::COMPATIBILITY_REVISION),
                            (Comparator, crate::comparator_law::COMPARATOR_LAW_REVISION),
                            (Observer, crate::observer_law::OBSERVER_LAW_REVISION),
                            (Lamp, crate::lamp_law::LAMP_LAW_IDS[0]),
                        ]
                        .map(|(role, id)| (role, id.into())),
                    );
                    (
                        InitializationPolicy::CompatibilityDeviceSeeds,
                        InputPolicy::ExplicitCompatibilityMutations,
                        Some(SchedulerProfile::default()),
                        Some(128),
                        None,
                    )
                }
                BoundedRedstoneEventsV1 => {
                    laws.insert(Repeater, crate::repeater_law::BOUNDED_REVISION.into());
                    laws.insert(Lamp, crate::lamp_law::LAMP_LAW_IDS[1].into());
                    laws.extend(
                        [PistonState, PistonMotion, PistonConnection, PistonPayload]
                            .into_iter()
                            .zip(crate::piston_law::PISTON_LAW_IDS)
                            .map(|(role, id)| (role, id.into())),
                    );
                    (
                        InitializationPolicy::SuppliedBlocksEmptyQueue,
                        InputPolicy::ExplicitScheduledWorldEvents,
                        Some(SchedulerProfile::default()),
                        None,
                        Some(crate::piston_law::builtin_piston_laws().default_motion_profile()),
                    )
                }
                UnifiedPistonElectricalCallbacksJava12111V19 => {
                    laws.retain(|role, _| *role == BlockTraits);
                    laws.insert(
                        ComparatorSignal,
                        crate::device_callback_law::COMPARATOR_SIGNAL_ID.into(),
                    );
                    laws.insert(DustStrength, crate::dust_law::DUST_LAW_REVISION.into());
                    for (role, id) in [SignalEmission, ConductorPower, PistonConnection, Repeater]
                        .into_iter()
                        .zip(crate::piston_electrical_law::LAW_IDS)
                    {
                        laws.insert(role, id.into());
                    }
                    laws.extend(DEVICE_LAWS.map(|(role, id)| (role, id.into())));
                    laws.insert(
                        PistonPayload,
                        crate::piston_law::ELECTRICAL_PAYLOAD_LAW.into(),
                    );
                    laws.extend(
                        [PistonControl, PistonCarrier, PistonGeometry, PistonHead]
                            .into_iter()
                            .zip(crate::piston_motion_law::LAW_IDS)
                            .map(|(role, id)| (role, id.into())),
                    );
                    (
                        InitializationPolicy::FreshElectricalPistonConstruction,
                        InputPolicy::PhysicalDeviceUsesBetweenSynchronousCalls,
                        None,
                        None,
                        None,
                    )
                }
            };
        Self {
            profile,
            laws,
            initialization,
            inputs,
            scheduler,
            max_electrical_iterations,
            piston_motion,
        }
    }

    pub fn is_proof_profile(&self) -> bool {
        matches!(
            self.profile,
            WorldExecutionProfile::DustTorchSynchronousGameTickV1
                | WorldExecutionProfile::DustSingleTorchBlockEffectsV1
        )
    }

    /// The fixed integration program is part of this profile's immutable
    /// contract, alongside its selected finite laws. It is not user code.
    pub fn device_program_revision(&self) -> Option<&'static str> {
        matches!(
            self.profile,
            WorldExecutionProfile::UnifiedPistonElectricalCallbacksJava12111V19
        )
        .then_some(crate::device_program::REVISION)
    }

    /// New callback delivery is an explicit profile dependency; the old flat
    /// scheduler field and serialized contexts retain their previous meaning.
    pub fn synchronous_runtime_profile(&self) -> Option<&'static str> {
        matches!(
            self.profile,
            WorldExecutionProfile::UnifiedPistonElectricalCallbacksJava12111V19
        )
        .then_some(crate::time::runtime::PROFILE)
    }

    /// Existing proof profiles select dust/torch bodies from their catalog.
    /// Other slots retain their exact native model pins. Accepting an arbitrary
    /// program in those slots requires a separately implemented world adapter.
    pub fn is_fixed_role(&self, role: LawRole) -> bool {
        !self.is_proof_profile() || !matches!(role, LawRole::DustStrength | LawRole::Torch)
    }

    pub fn validate(&self) -> Result<(), String> {
        let expected = Self::for_profile(self.profile);
        if self.laws.keys().ne(expected.laws.keys()) {
            return Err("execution profile requires its exact set of law roles".into());
        }
        for (role, id) in &self.laws {
            if id.trim().is_empty() || (self.is_fixed_role(*role) && id != &expected.laws[role]) {
                return Err(format!(
                    "execution profile requires the immutable law {:?} for {role:?}",
                    expected.laws[role]
                ));
            }
        }
        if self.initialization != expected.initialization || self.inputs != expected.inputs {
            return Err(
                "execution profile does not support these initial/input assumptions".into(),
            );
        }
        if self.scheduler.is_some() != expected.scheduler.is_some()
            || self.piston_motion.is_some() != expected.piston_motion.is_some()
            || self.max_electrical_iterations.is_some()
                != expected.max_electrical_iterations.is_some()
        {
            return Err("execution settings do not match the selected model".into());
        }
        // Compatibility construction has always used exactly 128 iterations.
        if !self.is_proof_profile()
            && self.max_electrical_iterations != expected.max_electrical_iterations
        {
            return Err("native compatibility solver retains its fixed iteration budget".into());
        }
        if let Some(profile) = self.scheduler {
            profile.validate().map_err(|e| e.to_string())?;
        }
        if let Some(profile) = self.piston_motion {
            profile.validate().map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/// The bounded world's stateless executable adapters are shared once. Actual
/// block state, reservations, histories and events remain in their world.
pub struct BoundedWorldLaws {
    pub dust: &'static crate::dust_law::DustStrengthLaw,
    pub spatial: &'static crate::spatial::SpatialLaws,
    pub repeater: &'static crate::repeater_law::BoundedRepeaterLaw,
    pub lamp: &'static crate::lamp_law::BoundedLampLaw,
    pub piston: &'static crate::piston_law::PistonLaws,
}

pub fn bounded_world_laws() -> &'static BoundedWorldLaws {
    static LAWS: OnceLock<BoundedWorldLaws> = OnceLock::new();
    LAWS.get_or_init(|| {
        WorldExecutionContext::for_profile(WorldExecutionProfile::BoundedRedstoneEventsV1)
            .validate()
            .expect("bounded world law pins");
        BoundedWorldLaws {
            dust: crate::dust_law::builtin_dust_law(),
            spatial: crate::spatial::builtin_spatial_laws(),
            repeater: crate::repeater_law::builtin_bounded_law(),
            lamp: crate::lamp_law::builtin_bounded_law(),
            piston: crate::piston_law::builtin_piston_laws(),
        }
    })
}
