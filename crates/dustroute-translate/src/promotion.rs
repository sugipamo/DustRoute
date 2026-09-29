//! Contextual review, followed by explicit adoption. Snapshot checks cover
//! placement, ports and routes. An explicit execution context additionally
//! checks supported behavioral obligations, never live Minecraft conformance.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use dustroute_library::PortDirection;
use dustroute_library::assembly::{Assembly, AssemblyPortRef, AssemblyView, BlueprintGrouping};
use dustroute_library::behavior_context::BehaviorReviewContext;
use dustroute_library::behavior_type::PhysicalBehaviorContext;
use dustroute_library::blueprint::{
    BlueprintCatalog, BlueprintError, BlueprintPort, BlueprintPortKind, BlueprintRevision,
    BlueprintRevisionId, InstancePath, TypeContract, TypeRevisionId,
};

use crate::behavior_review::BehaviorReview;
use crate::behavior_type::BehaviorBudget;
use crate::blueprint_connection::check_port_connection;
use crate::{world::BlockKind, world::Pos, world::ValidatedWorld, world::WorldValidationIssue};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Passed,
    Failed,
    Undetermined,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    Placement,
    Port,
    Connection,
    SourceRequirement,
    StaticType,
    PhysicalLaw,
    SourceConnection,
    Behavior,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CheckResult {
    pub kind: CheckKind,
    pub status: CheckStatus,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OccurrenceReview {
    pub revision: BlueprintRevisionId,
    pub checks: Vec<CheckResult>,
}

impl OccurrenceReview {
    #[must_use]
    pub fn status(&self) -> CheckStatus {
        aggregate(self.checks.iter().map(|check| check.status))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromotionReport {
    pub occurrences: BTreeMap<InstancePath, OccurrenceReview>,
    /// Includes unclassified surrounding geometry and assembly-wide routes.
    pub arrangement: Vec<CheckResult>,
    pub behavior_context: Option<BehaviorReviewContext>,
    pub behavior: Vec<CheckResult>,
}

impl PromotionReport {
    pub fn placement_validation_profile(&self) -> &'static str {
        if self
            .behavior_context
            .as_ref()
            .is_some_and(BehaviorReviewContext::is_runtime)
        {
            dustroute_minecraft::time::piston_runtime::ELECTRICAL_PROFILE
        } else {
            crate::world::ValidatedWorld::PROFILE
        }
    }
    /// Aggregates only the checks actually recorded, within their stated scope.
    #[must_use]
    pub fn status(&self) -> CheckStatus {
        aggregate(
            self.occurrences
                .values()
                .map(OccurrenceReview::status)
                .chain(self.arrangement.iter().map(|check| check.status))
                .chain(self.behavior.iter().map(|check| check.status)),
        )
    }

    /// None means no behavioral obligation was checked, not a vacuous pass.
    pub fn behavior_status(&self) -> Option<CheckStatus> {
        (!self.behavior.is_empty())
            .then(|| aggregate(self.behavior.iter().map(|check| check.status)))
    }

    fn record(&mut self, paths: impl IntoIterator<Item = InstancePath>, check: CheckResult) {
        for path in paths {
            self.occurrences
                .get_mut(&path)
                .expect("indexed occurrence")
                .checks
                .push(check.clone());
        }
        self.arrangement.push(check);
    }
}

pub(crate) fn aggregate(statuses: impl IntoIterator<Item = CheckStatus>) -> CheckStatus {
    statuses
        .into_iter()
        .fold(CheckStatus::Passed, |previous, next| {
            match (previous, next) {
                (CheckStatus::Failed, _) | (_, CheckStatus::Failed) => CheckStatus::Failed,
                (CheckStatus::Undetermined, _) | (_, CheckStatus::Undetermined) => {
                    CheckStatus::Undetermined
                }
                _ => CheckStatus::Passed,
            }
        })
}

fn result(kind: CheckKind, status: CheckStatus, detail: impl Into<String>) -> CheckResult {
    CheckResult {
        kind,
        status,
        detail: detail.into(),
    }
}

fn translated(position: Pos, offset: Pos) -> Result<Pos, BlueprintError> {
    Ok(Pos::new(
        position
            .x
            .checked_add(offset.x)
            .ok_or(BlueprintError::CoordinateOverflow)?,
        position
            .y
            .checked_add(offset.y)
            .ok_or(BlueprintError::CoordinateOverflow)?,
        position
            .z
            .checked_add(offset.z)
            .ok_or(BlueprintError::CoordinateOverflow)?,
    ))
}

fn port_type_check(
    catalog: &BlueprintCatalog,
    view: &AssemblyView,
    source_reference: &AssemblyPortRef,
    requirements: &[TypeRevisionId],
    kind: CheckKind,
) -> Result<CheckResult, BlueprintError> {
    let (source, rotation) = view.resolved_port(source_reference)?;
    let mut offsets = BTreeSet::from([Pos::default()]);
    for id in requirements {
        let definition = catalog
            .type_revision(id)
            .ok_or_else(|| BlueprintError::UnknownType(id.clone()))?;
        if let TypeContract::BlockPattern { blocks } = &definition.contract {
            offsets.extend(blocks.iter().map(|record| record.position));
        }
    }
    let mut states = BTreeMap::new();
    let mut unknown = Vec::new();
    for offset in offsets {
        let rotated = rotation
            .checked_pos(offset)
            .ok_or(BlueprintError::CoordinateOverflow)?;
        let position = translated(source.position, rotated)?;
        let Some(block) = view.block_at(position) else {
            unknown.push(position);
            continue;
        };
        if !rotation.is_identity() && block.piston_entity.is_some() {
            return Ok(result(
                kind,
                CheckStatus::Undetermined,
                "rotating block entities is outside the supported scope",
            ));
        }
        states.insert(
            offset,
            rotation
                .inverse()
                .checked_block(&block)
                .ok_or(BlueprintError::CoordinateOverflow)?,
        );
    }
    // A known contradiction is still a failure when another part is unknown.
    for id in requirements {
        let contradicted = match &catalog.type_revision(id).expect("checked type").contract {
            TypeContract::Signal { port_kind } => source.kind != *port_kind,
            TypeContract::BlockKind { block_kind } => states
                .get(&Pos::default())
                .is_some_and(|block| block.kind != *block_kind),
            TypeContract::BlockPattern { blocks } => blocks.iter().any(|record| {
                states
                    .get(&record.position)
                    .is_some_and(|actual| actual != &record.block)
            }),
            TypeContract::RepeatedSettling { .. }
            | TypeContract::PistonDoor { .. }
            | TypeContract::SingleOperation { .. }
            | TypeContract::Periodic { .. }
            | TypeContract::FiniteBurst { .. } => false,
        };
        if contradicted {
            return Ok(result(
                kind,
                CheckStatus::Failed,
                format!("{}: unsatisfied type {id}", source.name),
            ));
        }
    }
    if !unknown.is_empty() {
        return Ok(result(
            kind,
            CheckStatus::Undetermined,
            format!("unknown producer states at {unknown:?}"),
        ));
    }
    let snapshot: Vec<_> = requirements
        .iter()
        .filter(|id| {
            matches!(
                catalog.type_revision(id).expect("checked type").contract,
                TypeContract::Signal { .. }
                    | TypeContract::BlockKind { .. }
                    | TypeContract::BlockPattern { .. }
            )
        })
        .cloned()
        .collect();
    match catalog.check_port_types(&source, &snapshot, |offset| states.get(&offset).cloned()) {
        Ok(()) => Ok(result(
            kind,
            CheckStatus::Passed,
            format!("{}: actual terminal satisfies {snapshot:?}", source.name),
        )),
        Err(error) => Ok(result(
            kind,
            CheckStatus::Failed,
            format!("{}: {error}", source.name),
        )),
    }
}

fn requirement_check(
    catalog: &BlueprintCatalog,
    view: &AssemblyView,
    source_reference: &AssemblyPortRef,
    consumer: &BlueprintPort,
    behavior: &mut BehaviorReview<'_>,
) -> Result<CheckResult, BlueprintError> {
    let (source, _) = view.resolved_port(source_reference)?;
    if source.direction != PortDirection::Output
        || consumer.direction != PortDirection::Input
        || ((source.kind == BlueprintPortKind::BlockState)
            != (consumer.kind == BlueprintPortKind::BlockState))
    {
        return Ok(result(
            CheckKind::SourceRequirement,
            CheckStatus::Failed,
            "incompatible producer and consumer interfaces",
        ));
    }
    let snapshot = port_type_check(
        catalog,
        view,
        source_reference,
        &consumer.required_source_types,
        CheckKind::SourceRequirement,
    )?;
    if snapshot.status != CheckStatus::Passed {
        return Ok(snapshot);
    }
    let checks: Vec<_> = consumer
        .required_source_types
        .iter()
        .filter(|id| {
            matches!(
                catalog.type_revision(id).expect("checked type").contract,
                TypeContract::RepeatedSettling { .. }
                    | TypeContract::PistonDoor { .. }
                    | TypeContract::SingleOperation { .. }
                    | TypeContract::Periodic { .. }
                    | TypeContract::FiniteBurst { .. }
            )
        })
        .map(|id| behavior.check_source(id, source_reference, view))
        .collect();
    Ok(result(
        CheckKind::SourceRequirement,
        aggregate(checks.iter().map(|check| check.status)),
        std::iter::once(snapshot.detail.as_str())
            .chain(checks.iter().map(|check| check.detail.as_str()))
            .collect::<Vec<_>>()
            .join("; "),
    ))
}

/// Checks every occurrence in the complete proposed context, including shared
/// interpretations. A parent's result never overwrites a descendant's result.
pub fn review_assembly(
    catalog: &BlueprintCatalog,
    assembly: &Assembly,
) -> Result<PromotionReport, BlueprintError> {
    review_assembly_in_context(catalog, assembly, None, BehaviorBudget::default())
}

/// Checks each declared behavioral obligation against the complete physical
/// context. An omitted context leaves behavior undetermined, never assumed.
pub fn review_assembly_in_context(
    catalog: &BlueprintCatalog,
    assembly: &Assembly,
    context: Option<&PhysicalBehaviorContext>,
    budget: BehaviorBudget,
) -> Result<PromotionReport, BlueprintError> {
    let view = assembly.inspect(catalog)?;
    let world_laws = context.map(|context| crate::world_laws::WorldLaws::resolve(catalog, context));
    let mut behavior = BehaviorReview::new(catalog, assembly, context, budget);
    let raw_world = view.proposed_world();
    let mut report = PromotionReport {
        occurrences: view.occurrences.iter().map(|(path, occurrence)| (path.clone(), OccurrenceReview {
            revision: occurrence.revision.clone(),
            checks: vec![result(CheckKind::Placement, CheckStatus::Passed,
                "placement checks against composed state; source differences are not equality requirements")],
        })).collect(),
        arrangement: vec![],
        behavior_context: context.cloned().map(Into::into),
        behavior: vec![],
    };
    for issue in raw_world.placement_issues_with_lookup(|pos| view.block_at(pos)) {
        let (position, unknown) = match &issue {
            WorldValidationIssue::InvalidSupport {
                position, support, ..
            } => (
                Some(*position),
                support.is_some_and(|pos| view.block_at(pos).is_none()),
            ),
            WorldValidationIssue::UnsupportedPlacement { position, .. } => (Some(*position), true),
            WorldValidationIssue::InvalidState { position, .. }
            | WorldValidationIssue::InvalidWireConnection { position, .. }
            | WorldValidationIssue::DuplicateChange { position } => (Some(*position), false),
            WorldValidationIssue::UnknownWireConnection { position, .. } => (Some(*position), true),
            WorldValidationIssue::SyntheticInputDriver { .. } => (None, false),
        };
        let paths = position
            .and_then(|pos| view.membership.get(&pos))
            .cloned()
            .unwrap_or_default();
        report.record(
            paths,
            result(
                CheckKind::Placement,
                if unknown {
                    CheckStatus::Undetermined
                } else {
                    CheckStatus::Failed
                },
                format!("{issue:?}"),
            ),
        );
    }
    // Unknown portions of an occurrence cannot be certified from a sparse world.
    for (position, claims) in &view.source_claims {
        let actual = view.block_at(*position);
        if actual.is_none() {
            report.record(
                claims.keys().cloned(),
                result(
                    CheckKind::Placement,
                    CheckStatus::Undetermined,
                    format!("unknown claimed position {position:?}"),
                ),
            );
        }
        for (path, source) in claims {
            if source.kind == BlockKind::Air
                && actual
                    .as_ref()
                    .is_some_and(|block| block.kind != BlockKind::Air)
            {
                report.record(
                    [path.clone()],
                    result(
                        CheckKind::Placement,
                        CheckStatus::Failed,
                        format!("explicit air requirement at {position:?} is occupied"),
                    ),
                );
            }
        }
    }
    let world = ValidatedWorld::try_from(raw_world).ok();
    let mut connections = BTreeMap::new();
    let mut route_statuses = BTreeMap::new();
    for edge in &assembly.connections {
        let key = view.connection_key(edge)?;
        connections
            .entry(key.clone())
            .or_insert_with(Vec::new)
            .push(edge);
        let (source, rotation) = view.resolved_port(&edge.source)?;
        let (mut sink, _) = view.resolved_port(&edge.sink)?;
        // Type requirements are attributed to the declaring consumer below,
        // independently of the shared terminal's route and other aliases.
        sink.required_source_types.clear();
        let check = if edge.path.iter().any(|pos| view.block_at(*pos).is_none()) {
            result(
                CheckKind::Connection,
                CheckStatus::Undetermined,
                "route crosses unknown space",
            )
        } else if source.kind == BlueprintPortKind::BlockState
            || sink.kind == BlueprintPortKind::BlockState
        {
            result(
                CheckKind::Connection,
                CheckStatus::Undetermined,
                "block-state routes are not supported yet",
            )
        } else if let Some(world) = &world {
            match check_port_connection(
                catalog,
                world,
                &source,
                &sink,
                &edge.path,
                rotation,
                |pos| view.block_at(pos),
            ) {
                Ok(()) => result(
                    CheckKind::Connection,
                    CheckStatus::Passed,
                    format!("{key:?}"),
                ),
                Err(error) => result(
                    CheckKind::Connection,
                    CheckStatus::Failed,
                    format!("{key:?}: {error}"),
                ),
            }
        } else {
            result(
                CheckKind::Connection,
                CheckStatus::Undetermined,
                "route check requires a valid placement",
            )
        };
        route_statuses
            .entry(key.clone())
            .or_insert_with(Vec::new)
            .push(check.status);
        report.record(
            BTreeSet::from([key.0.instance.clone(), key.1.instance.clone()]),
            check,
        );
    }
    for (path, occurrence) in &view.occurrences {
        let revision = catalog
            .revision(&occurrence.revision)
            .expect("indexed source");
        if !revision.required_laws.is_empty() {
            let check = match context {
                None => result(
                    CheckKind::PhysicalLaw,
                    CheckStatus::Undetermined,
                    "physical law requirements need an explicit world execution context",
                ),
                Some(context) => match crate::world_laws::check_requirements(
                    catalog,
                    context,
                    &revision.required_laws,
                ) {
                    Err(error) => result(CheckKind::PhysicalLaw, CheckStatus::Failed, error),
                    Ok(()) => match world_laws.as_ref().expect("context supplied") {
                        Err(error) => result(
                            CheckKind::PhysicalLaw,
                            CheckStatus::Undetermined,
                            error.clone(),
                        ),
                        Ok(_) => result(
                            CheckKind::PhysicalLaw,
                            CheckStatus::Passed,
                            format!(
                                "world-selected laws satisfy {:?}; declaration creates no execution state",
                                revision.required_laws
                            ),
                        ),
                    },
                },
            };
            report
                .occurrences
                .get_mut(path)
                .expect("indexed occurrence")
                .checks
                .push(check);
        }
        for definition in &revision.ports {
            let reference = AssemblyPortRef {
                instance: path.clone(),
                port: definition.name.clone(),
            };
            let port = view.port(catalog, &reference)?;
            let status = match view.block_at(port.position) {
                None => CheckStatus::Undetermined,
                Some(block) => match port.kind {
                    BlueprintPortKind::Wire if block.kind == BlockKind::RedstoneWire => {
                        CheckStatus::Passed
                    }
                    BlueprintPortKind::BlockPower
                        if block.redstone_traits().conducts_weak_power
                            || block.redstone_traits().conducts_strong_power =>
                    {
                        CheckStatus::Passed
                    }
                    BlueprintPortKind::DeviceOutput if port.kind.matches_signal_block(&block) => {
                        CheckStatus::Passed
                    }
                    BlueprintPortKind::BlockState => CheckStatus::Undetermined,
                    _ => CheckStatus::Failed,
                },
            };
            report.record(
                [path.clone()],
                result(
                    CheckKind::Port,
                    status,
                    format!("{reference:?} at {:?}", port.position),
                ),
            );
            if port.direction != PortDirection::Input || port.required_source_types.is_empty() {
                continue;
            }
            let canonical = view.canonical_port_ref(&reference)?;
            let producers: Vec<_> = connections
                .iter()
                .filter(|((_, sink), _)| sink == canonical)
                .flat_map(|(_, edges)| edges.iter().copied())
                .collect();
            if producers.is_empty() {
                report.record(
                    [path.clone()],
                    result(
                        CheckKind::SourceRequirement,
                        CheckStatus::Undetermined,
                        format!("no producer supplied for {reference:?}"),
                    ),
                );
            }
            for edge in producers {
                report.record(
                    [path.clone()],
                    requirement_check(catalog, &view, &edge.source, &port, &mut behavior)?,
                );
            }
        }
        for binding in &revision.static_type_bindings {
            let reference = AssemblyPortRef {
                instance: path.clone(),
                port: binding.port.clone(),
            };
            let check = port_type_check(
                catalog,
                &view,
                &reference,
                std::slice::from_ref(&binding.type_revision),
                CheckKind::StaticType,
            )?;
            report
                .occurrences
                .get_mut(path)
                .expect("indexed occurrence")
                .checks
                .push(check);
        }
        for binding in &revision.behavior_bindings {
            report.record([path.clone()], behavior.check_binding(binding, path, &view));
        }
        let prefix = |reference: &AssemblyPortRef| AssemblyPortRef {
            instance: path.iter().chain(&reference.instance).cloned().collect(),
            port: reference.port.clone(),
        };
        for edge in &revision.connections {
            let key = (
                view.canonical_port_ref(&prefix(&edge.source))?.clone(),
                view.canonical_port_ref(&prefix(&edge.sink))?.clone(),
            );
            let status = route_statuses
                .get(&key)
                .map_or(CheckStatus::Failed, |statuses| {
                    aggregate(statuses.iter().copied())
                });
            report.record(
                [path.clone()],
                result(
                    CheckKind::SourceConnection,
                    status,
                    format!("declared connection {key:?}"),
                ),
            );
        }
    }
    report.behavior = behavior.checks;
    Ok(report)
}

/// Shared review entry for the existing proposal/adoption workflow. The native
/// branch uses its own initial gate and complete-world exploration; it never
/// manufactures a legacy `ValidatedWorld` for moving geometry.
pub fn review_assembly_with_context(
    catalog: &BlueprintCatalog,
    assembly: &Assembly,
    context: Option<&BehaviorReviewContext>,
    budget: BehaviorBudget,
) -> Result<PromotionReport, BlueprintError> {
    match context {
        None => review_assembly_in_context(catalog, assembly, None, budget),
        Some(BehaviorReviewContext::FixedGeometry(context)) => {
            review_assembly_in_context(catalog, assembly, Some(context), budget)
        }
        Some(BehaviorReviewContext::Runtime(runtime_context)) => {
            let native = crate::runtime_review::review_assembly_in_runtime_context(
                catalog,
                assembly,
                runtime_context,
                budget,
            )?;
            let behavior = native
                .occurrences
                .values()
                .flat_map(|occurrence| &occurrence.checks)
                .filter(|check| check.kind == CheckKind::Behavior)
                .cloned()
                .collect();
            Ok(PromotionReport {
                occurrences: native.occurrences,
                arrangement: native.arrangement,
                behavior_context: context.cloned(),
                behavior,
            })
        }
    }
}

/// Editable data is created separately; this candidate owns an immutable copy.
#[derive(Clone, Debug)]
pub struct PromotionCandidate {
    blueprint: BlueprintRevision,
    assembly: Assembly,
}

impl PromotionCandidate {
    pub fn prepare(
        catalog: &BlueprintCatalog,
        assembly: &Assembly,
        grouping: BlueprintGrouping,
    ) -> Result<Self, BlueprintError> {
        let (blueprint, assembly) = assembly.group_as_blueprint(catalog, grouping)?;
        Ok(Self {
            blueprint,
            assembly,
        })
    }

    #[must_use]
    pub fn blueprint(&self) -> &BlueprintRevision {
        &self.blueprint
    }

    #[must_use]
    pub fn assembly(&self) -> &Assembly {
        &self.assembly
    }

    /// No catalog mutation. Failed and undetermined reviews retain their data.
    pub fn validate(&self, catalog: &BlueprintCatalog) -> Result<PromotionReview, BlueprintError> {
        self.validate_inner(catalog, None, BehaviorBudget::default())
    }

    pub fn validate_in_context(
        &self,
        catalog: &BlueprintCatalog,
        context: impl Into<BehaviorReviewContext>,
        budget: BehaviorBudget,
    ) -> Result<PromotionReview, BlueprintError> {
        self.validate_inner(catalog, Some(context.into()), budget)
    }

    fn validate_inner(
        &self,
        catalog: &BlueprintCatalog,
        context: Option<BehaviorReviewContext>,
        budget: BehaviorBudget,
    ) -> Result<PromotionReview, BlueprintError> {
        let mut proposed = catalog.clone();
        proposed.insert_revision(self.blueprint.clone())?;
        let report =
            review_assembly_with_context(&proposed, &self.assembly, context.as_ref(), budget)?;
        Ok(PromotionReview {
            candidate: self.clone(),
            report,
            catalog: proposed,
            context,
            budget,
        })
    }
}

#[derive(Clone, Debug)]
pub struct PromotionReview {
    candidate: PromotionCandidate,
    report: PromotionReport,
    catalog: BlueprintCatalog,
    context: Option<BehaviorReviewContext>,
    budget: BehaviorBudget,
}

#[derive(Clone, Debug)]
pub enum PromotionError {
    Blueprint(BlueprintError),
    Validation(Box<PromotionReport>),
    ChangedDependency(BlueprintRevisionId),
}

impl std::fmt::Display for PromotionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl std::error::Error for PromotionError {}
impl From<BlueprintError> for PromotionError {
    fn from(error: BlueprintError) -> Self {
        Self::Blueprint(error)
    }
}

impl PromotionReview {
    #[must_use]
    pub fn report(&self) -> &PromotionReport {
        &self.report
    }

    #[must_use]
    pub fn candidate(&self) -> &PromotionCandidate {
        &self.candidate
    }

    /// Explicit adoption, with no edit access to the reviewed candidate. This
    /// appends a source definition only; placement in Minecraft is a separate act.
    pub fn adopt(&self, catalog: &mut BlueprintCatalog) -> Result<Assembly, PromotionError> {
        if self.report.status() != CheckStatus::Passed {
            return Err(PromotionError::Validation(Box::new(self.report.clone())));
        }
        if let Some(context) = &self.context {
            for id in context.law_revisions() {
                if catalog.revision(&id) != self.catalog.revision(&id) {
                    return Err(PromotionError::ChangedDependency(id));
                }
            }
        }
        let view = self.candidate.assembly.inspect(&self.catalog)?;
        for occurrence in view.occurrences.values() {
            if occurrence.revision == self.candidate.blueprint.id {
                continue;
            }
            let expected = self
                .catalog
                .revision(&occurrence.revision)
                .expect("reviewed source");
            if catalog.revision(&occurrence.revision) != Some(expected) {
                return Err(PromotionError::ChangedDependency(
                    occurrence.revision.clone(),
                ));
            }
        }
        // Definitions for connection types and labels are also pinned. Check
        // candidate root requirements as well as all descendant requirements.
        for occurrence in view.occurrences.values() {
            let source = self
                .catalog
                .revision(&occurrence.revision)
                .expect("reviewed source");
            if source
                .required_laws
                .iter()
                .any(|id| catalog.revision(id) != self.catalog.revision(id))
            {
                return Err(PromotionError::ChangedDependency(
                    occurrence.revision.clone(),
                ));
            }
            if source
                .ports
                .iter()
                .flat_map(|port| &port.required_source_types)
                .chain(
                    source
                        .behavior_bindings
                        .iter()
                        .map(|binding| binding.behavior_type()),
                )
                .chain(
                    source
                        .static_type_bindings
                        .iter()
                        .map(|binding| &binding.type_revision),
                )
                .any(|id| catalog.type_revision(id) != self.catalog.type_revision(id))
                || source
                    .classifications
                    .iter()
                    .any(|id| catalog.classification(id) != self.catalog.classification(id))
            {
                return Err(PromotionError::ChangedDependency(
                    occurrence.revision.clone(),
                ));
            }
        }
        let mut proposed = catalog.clone();
        proposed.insert_revision(self.candidate.blueprint.clone())?;
        // Retain the existing assembly gate as a second invariant of adoption.
        // It also guards against future divergence between detailed review and
        // the compiler's required validation path.
        crate::assembly::validate_assembly_for_adoption(
            &proposed,
            &self.candidate.assembly,
            self.context.as_ref(),
            self.budget,
        )
        .map_err(|error| PromotionError::Blueprint(BlueprintError::Invalid(error.to_string())))?;
        *catalog = proposed;
        Ok(self.candidate.assembly.clone())
    }
}
