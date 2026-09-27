//! Resolve a world's exact law dependencies without executing interpretations.
use std::collections::{BTreeMap, BTreeSet};

use dustroute_minecraft::execution_context::{LawRole, WorldExecutionContext};

use crate::blueprint::{BlueprintCatalog, BlueprintRevision, BlueprintRevisionId};
use crate::builtin_laws::builtin_laws;

pub fn resolve_law_references<'a>(
    catalog: &'a BlueprintCatalog,
    context: &WorldExecutionContext,
) -> Result<BTreeMap<LawRole, &'a BlueprintRevision>, String> {
    context.validate()?;
    let mut selected = BTreeMap::new();
    for (role, name) in &context.laws {
        let id = BlueprintRevisionId::new(name.clone()).map_err(|e| e.to_string())?;
        let source = catalog
            .revision(&id)
            .or_else(|| {
                context
                    .is_fixed_role(*role)
                    .then(|| builtin_laws().revision(&id))
                    .flatten()
            })
            .ok_or_else(|| format!("unknown physical law {id}"))?;
        if context.is_fixed_role(*role) && Some(source) != builtin_laws().revision(&id) {
            let family = if matches!(
                role,
                LawRole::BlockTraits
                    | LawRole::WireShape
                    | LawRole::WireTransfer
                    | LawRole::WireWeakPower
            ) {
                "spatial law"
            } else {
                "law"
            };
            return Err(format!(
                "world profile requires the immutable {family} {id}"
            ));
        }
        if source.law.is_none() {
            return Err(format!("physical law {id} has no executable program"));
        }
        selected.insert(*role, source);
    }
    check_selected_requirements(&selected, selected.values().map(|source| &source.id))?;
    Ok(selected)
}

fn check_selected_requirements<'a>(
    selected: &BTreeMap<LawRole, &'a BlueprintRevision>,
    required: impl IntoIterator<Item = &'a BlueprintRevisionId>,
) -> Result<(), String> {
    let by_id: BTreeMap<_, _> = selected.values().map(|s| (&s.id, *s)).collect();
    let mut pending: Vec<_> = required.into_iter().collect();
    let mut checked = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !checked.insert(id) {
            continue;
        }
        let source = by_id.get(id).ok_or_else(|| {
            format!("required physical law {id} is not selected by the world context")
        })?;
        pending.extend(&source.required_laws);
    }
    Ok(())
}

pub fn check_law_requirements(
    catalog: &BlueprintCatalog,
    context: &WorldExecutionContext,
    required: &[BlueprintRevisionId],
) -> Result<(), String> {
    // Preserve a missing selection as a conflict, independently of whether
    // another selected adapter will later prove executable.
    for id in required {
        if !context
            .laws
            .values()
            .any(|selected| selected == id.as_str())
        {
            return Err(format!(
                "required physical law {id} is not selected by the world context"
            ));
        }
    }
    // This check is about dependency selection, not adapter support. Review
    // reports incompatible dependencies as Failed, but an unsupported selected
    // adapter as Undetermined through resolve_law_references / compilation.
    let selected = context
        .laws
        .iter()
        .filter_map(|(role, name)| {
            let id = BlueprintRevisionId::new(name.clone()).ok()?;
            let source = catalog.revision(&id).or_else(|| {
                context
                    .is_fixed_role(*role)
                    .then(|| builtin_laws().revision(&id))
                    .flatten()
            })?;
            Some((*role, source))
        })
        .collect();
    check_selected_requirements(&selected, required)
}
