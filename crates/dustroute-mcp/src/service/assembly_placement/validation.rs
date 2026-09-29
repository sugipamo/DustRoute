//! Fresh source/adoption identity and target execution checks.
use super::*;

pub(super) fn server_contract(
    status: &crate::bridge::BotStatus,
    dimension: &str,
) -> Result<(), String> {
    if !status.connected
        || status.version != "1.21.11"
        || status.dimension.as_deref() != Some(dimension)
    {
        return Err(
            "custom piston construction requires the connected Java 1.21.11 target dimension"
                .into(),
        );
    }
    match status.enabled_features.as_deref() {
        Some([feature]) if feature == "minecraft:vanilla" => Ok(()),
        _ => Err("construction requires observed vanilla feature flags; experimental or unavailable settings are not verified".into()),
    }
}

pub(super) fn proof_from_basis(
    basis: &crate::source_identity::SourceIdentity,
    transform: AssemblyTransform,
) -> Result<ValidatedAssemblyPlacement, String> {
    ValidatedAssemblyPlacement::review_target(
        &basis.catalog,
        &basis.record.assembly,
        &basis.context,
        transform,
    )
}
