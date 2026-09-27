//! Explicit ownership for an isolated component search. Physics always uses
//! the complete Assembly; excluding a block from cost does not remove its effects.
use super::*;

#[derive(Clone, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BlueprintReductionScope {
    /// Compatibility mode: optimize and count the complete supplied Assembly.
    #[default]
    WholeAssembly,
    /// These occupied cells initially belong to the body. Other occupied cells
    /// are fixed external equipment. New body positions may use known empty space.
    /// Environmental interpretations are explicit and all are freshly checked.
    /// External routes and boundaries must remain resolvable through retained
    /// environment occurrences; otherwise explicit parent reconnection is needed.
    Component {
        body_positions: Vec<Pos>,
        environment_instances: Vec<BlueprintInclusion>,
    },
}

#[derive(Default)]
pub(super) struct SearchScope {
    pub component: bool,
    pub external: BTreeMap<Pos, Block>,
    pub environment_instances: Vec<BlueprintInclusion>,
    pub environment_connections: Vec<BlueprintConnection>,
    pub environment_boundaries: Vec<BlueprintPortBinding>,
}

impl SearchScope {
    pub fn resolve(
        catalog: &BlueprintCatalog,
        request: &BlueprintReductionRequest,
        base: &AssemblyRevision,
    ) -> Result<Self, String> {
        let BlueprintReductionScope::Component {
            body_positions,
            environment_instances,
        } = &request.scope
        else {
            return Ok(Self::default());
        };
        let body: BTreeSet<_> = body_positions.iter().copied().collect();
        if body.is_empty() || body.len() != body_positions.len() {
            return Err("component body needs distinct occupied positions".into());
        }
        let occupied: BTreeMap<_, _> = base
            .assembly
            .blocks
            .iter()
            .filter(|b| b.block.kind != BlockKind::Air)
            .map(|b| (b.position, b.block.clone()))
            .collect();
        if body.iter().any(|p| !occupied.contains_key(p)) {
            return Err("body positions must be actual occupied cells".into());
        }
        if request
            .behavior_context
            .input_drivers
            .iter()
            .any(|driver| body.contains(&driver.lever_position))
        {
            return Err(
                "component input controls belong to external equipment, not the body".into(),
            );
        }
        if environment_instances
            .iter()
            .any(|i| i.instance.as_str() == "optimized")
        {
            return Err("environment instance name optimized is reserved for the candidate".into());
        }
        let retained = |reference: &BlueprintPortRef| {
            reference
                .instance
                .first()
                .is_some_and(|root| environment_instances.iter().any(|i| &i.instance == root))
        };
        let environment_connections: Vec<_> = base
            .assembly
            .connections
            .iter()
            .filter(|edge| {
                retained(&edge.source)
                    || retained(&edge.sink)
                    || edge.path.iter().any(|p| !body.contains(p))
            })
            .cloned()
            .collect();
        // Moving a body terminal may require a new parent route. Search has no
        // authority to invent that reconnection or silently discard its contract.
        if environment_connections
            .iter()
            .any(|edge| !retained(&edge.source) || !retained(&edge.sink))
        {
            return Err("external routes require retained environment endpoints; prepare explicit parent reconnection before component search".into());
        }
        let view = base.assembly.inspect(catalog).map_err(|e| e.to_string())?;
        let mut environment_boundaries = Vec::new();
        for boundary in &base.assembly.boundaries {
            let (port, _) = view
                .resolved_port(&boundary.port)
                .map_err(|e| e.to_string())?;
            if retained(&boundary.port) || !body.contains(&port.position) {
                if !retained(&boundary.port) {
                    return Err("external boundaries require retained environment endpoints; prepare explicit parent reconnection before component search".into());
                }
                environment_boundaries.push(boundary.clone());
            }
        }
        Ok(Self {
            component: true,
            external: occupied
                .into_iter()
                .filter(|(p, _)| !body.contains(p))
                .collect(),
            environment_instances: environment_instances.clone(),
            environment_connections,
            environment_boundaries,
        })
    }

    pub fn owns(&self, position: Pos) -> bool {
        !self.external.contains_key(&position)
    }

    /// The body is defined by the candidate's composed source membership and
    /// counted from actual cells, never by source block states or metrics.
    pub fn counts(
        &self,
        catalog: &BlueprintCatalog,
        candidate: &BlueprintReductionCandidate,
    ) -> Result<(usize, usize), String> {
        let mut staged = catalog.clone();
        staged
            .insert_revision(candidate.blueprint.clone())
            .map_err(|e| e.to_string())?;
        let assembly = &candidate.state.assembly;
        let view = assembly.inspect(&staged).map_err(|e| e.to_string())?;
        let world = view.proposed_world();
        let total = world.iter().count();
        if !self.component {
            return Ok((total, total));
        }
        let roots: Vec<_> = assembly
            .instances
            .iter()
            .filter(|i| i.revision == candidate.blueprint.id)
            .collect();
        if roots.len() != 1 {
            return Err("component candidate must have exactly one body occurrence".into());
        }
        let root = &roots[0].instance;
        let retained: Vec<_> = assembly
            .instances
            .iter()
            .filter(|i| i.instance != *root)
            .cloned()
            .collect();
        if retained != self.environment_instances {
            return Err(
                "component candidate must retain the declared environment instances".into(),
            );
        }
        if self
            .environment_connections
            .iter()
            .any(|edge| !assembly.connections.contains(edge))
            || self
                .environment_boundaries
                .iter()
                .any(|boundary| !assembly.boundaries.contains(boundary))
        {
            return Err("component candidate must retain external routes and boundaries; reconnection requires an explicit parent proposal".into());
        }
        let body: BTreeSet<_> = view
            .membership
            .iter()
            .filter(|(_, paths)| paths.iter().any(|p| p.first() == Some(root)))
            .map(|(p, _)| *p)
            .collect();
        if self
            .external
            .iter()
            .any(|(p, block)| world.get(*p) != Some(block) || body.contains(p))
        {
            return Err("component candidate cannot modify or absorb external equipment".into());
        }
        if world
            .iter()
            .any(|(p, _)| !body.contains(p) && !self.external.contains_key(p))
        {
            return Err("every new occupied cell must be counted as part of the body".into());
        }
        for port in &candidate.blueprint.ports {
            let (resolved, _) = view
                .resolved_port(&BlueprintPortRef {
                    instance: vec![root.clone()],
                    port: port.name.clone(),
                })
                .map_err(|e| e.to_string())?;
            if !body.contains(&resolved.position) {
                return Err("component ports must belong to the body".into());
            }
        }
        if candidate
            .behavior_context
            .input_drivers
            .iter()
            .any(|d| !self.external.contains_key(&d.lever_position))
        {
            return Err("component verification controls must remain external".into());
        }
        let cost = world.iter().filter(|(p, _)| body.contains(p)).count();
        Ok((cost, total))
    }
}
