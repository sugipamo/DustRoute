//! Fresh source/adoption identity and target execution checks.
use super::*;

pub(super) fn source_identity(basis: &Value) -> Value {
    json!({"record":basis["record"], "adopted_by":basis["adopted_by"], "behavior_context":basis["behavior_context"], "catalog_json":basis["catalog_json"]})
}
// Catalogs append immutable records. Unrelated new adoptions must not strand
// existing instances, while any alteration/removal of captured data is refused.
pub(super) fn source_basis_matches(saved: &Value, current: &Value) -> Result<bool, String> {
    if ["record", "adopted_by", "behavior_context"]
        .iter()
        .any(|key| saved[*key] != current[*key])
    {
        return Ok(false);
    }
    let catalog = |value: &Value| {
        BlueprintCatalog::from_json(
            value["catalog_json"]
                .as_str()
                .ok_or("catalog basis missing")?,
        )
        .map_err(|e| e.to_string())
    };
    let old = catalog(saved)?;
    let new = catalog(current)?;
    Ok(old.revisions().all(|r| new.revision(&r.id) == Some(r))
        && old
            .type_revisions()
            .all(|r| new.type_revision(&r.id) == Some(r))
        && old
            .classifications()
            .all(|r| new.classification(&r.id) == Some(r))
        && old.assemblies().all(|r| new.assembly(&r.id) == Some(r)))
}
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

pub(super) struct ConstructionSource {
    pub(super) record: AssemblyRevision,
    pub(super) context: RuntimeBehaviorContext,
    pub(super) catalog: BlueprintCatalog,
}

impl ConstructionSource {
    pub(super) fn parse(basis: &Value) -> Result<Self, String> {
        let record = serde_json::from_value(basis["record"].clone()).map_err(|e| e.to_string())?;
        let context =
            serde_json::from_value(basis["behavior_context"].clone()).map_err(|e| e.to_string())?;
        let catalog = BlueprintCatalog::from_json(
            basis["catalog_json"]
                .as_str()
                .ok_or("catalog basis missing")?,
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            record,
            context,
            catalog,
        })
    }
}

pub(super) fn proof_from_basis(
    basis: &Value,
    transform: AssemblyTransform,
) -> Result<ValidatedAssemblyPlacement, String> {
    let ConstructionSource {
        record,
        context,
        catalog,
    } = ConstructionSource::parse(basis)?;
    ValidatedAssemblyPlacement::review_target(&catalog, &record.assembly, &context, transform)
}
