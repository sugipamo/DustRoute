//! Structural interface indexing shared by source definitions and placed state.
//! No state, route connectivity or logical meaning is certified here.

use crate::PortDirection;
use crate::blueprint::{
    BlueprintCatalog, BlueprintConnection, BlueprintError, BlueprintOccurrence, BlueprintPort,
    BlueprintPortRef, ExpansionLimits, InstancePath, TypeRevisionId, invalid, transformed, unique,
};
use dustroute_minecraft::RotationY;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct InterfaceIndex {
    ports: BTreeMap<BlueprintPortRef, (BlueprintPort, RotationY)>,
    canonical: BTreeMap<BlueprintPortRef, BlueprintPortRef>,
    requirements: BTreeMap<BlueprintPortRef, BTreeSet<TypeRevisionId>>,
    pub source_connections: Vec<BlueprintConnection>,
}

impl InterfaceIndex {
    pub fn new(
        catalog: &BlueprintCatalog,
        occurrences: &BTreeMap<InstancePath, BlueprintOccurrence>,
    ) -> Result<Self, BlueprintError> {
        let mut index = Self::default();
        let limit = ExpansionLimits::default().maximum_block_claims;
        let mut aliases = BTreeMap::new();
        let prefix = |path: &InstancePath, reference: &BlueprintPortRef| BlueprintPortRef {
            instance: path.iter().chain(&reference.instance).cloned().collect(),
            port: reference.port.clone(),
        };
        for (path, occurrence) in occurrences {
            let record = catalog
                .revision(&occurrence.revision)
                .ok_or_else(|| BlueprintError::UnknownRevision(occurrence.revision.clone()))?;
            if record.ports.len() > limit - index.ports.len() {
                return Err(BlueprintError::ExpansionLimit("interface records"));
            }
            if record.port_bindings.len() > record.ports.len() {
                return invalid("port bindings exceed the number of parent ports");
            }
            for port in &record.ports {
                let reference = BlueprintPortRef {
                    instance: path.clone(),
                    port: port.name.clone(),
                };
                let mut port = port.clone();
                port.position = transformed(port.position, occurrence.origin, occurrence.rotation)?;
                port.facing = port.facing.map(|facing| occurrence.rotation.facing(facing));
                if index
                    .ports
                    .insert(reference, (port, occurrence.rotation))
                    .is_some()
                {
                    return invalid("duplicate physical port");
                }
            }
            for binding in &record.port_bindings {
                if binding.port.instance.is_empty() {
                    return invalid("a port binding must expose a descendant terminal");
                }
                let source = BlueprintPortRef {
                    instance: path.clone(),
                    port: binding.name.clone(),
                };
                if aliases
                    .insert(source, prefix(path, &binding.port))
                    .is_some()
                {
                    return invalid("duplicate parent port binding");
                }
            }
        }
        for (source, target) in &aliases {
            let source_port = &index.port(source)?.0;
            let target_port = &index.port(target)?.0;
            if source_port.position != target_port.position
                || source_port.kind != target_port.kind
                || source_port.direction != target_port.direction
                || source_port.facing != target_port.facing
            {
                return invalid("a port binding must expose the same physical terminal");
            }
        }
        for reference in index.ports.keys() {
            let mut canonical = reference;
            // Every binding descends strictly within a bounded containment tree.
            while let Some(target) = aliases.get(canonical) {
                canonical = target;
            }
            index.canonical.insert(reference.clone(), canonical.clone());
            index
                .requirements
                .entry(canonical.clone())
                .or_default()
                .extend(
                    index.ports[reference]
                        .0
                        .required_source_types
                        .iter()
                        .cloned(),
                );
        }
        let mut steps = 0;
        for (path, occurrence) in occurrences {
            let record = catalog
                .revision(&occurrence.revision)
                .expect("indexed source exists");
            unique(
                record
                    .connections
                    .iter()
                    .map(|edge| (&edge.source, &edge.sink)),
                "duplicate source connection",
            )?;
            for edge in &record.connections {
                if edge.path.len() > limit - steps || index.source_connections.len() >= limit {
                    return Err(BlueprintError::ExpansionLimit("source connection records"));
                }
                steps += edge.path.len();
                let edge = BlueprintConnection {
                    source: prefix(path, &edge.source),
                    sink: prefix(path, &edge.sink),
                    path: edge
                        .path
                        .iter()
                        .map(|position| {
                            transformed(*position, occurrence.origin, occurrence.rotation)
                        })
                        .collect::<Result<_, _>>()?,
                };
                let source = &index.port(&edge.source)?.0;
                let sink = &index.port(&edge.sink)?.0;
                if source.direction != PortDirection::Output
                    || sink.direction != PortDirection::Input
                    || edge.path.first() != Some(&source.position)
                    || edge.path.last() != Some(&sink.position)
                {
                    return invalid(
                        "source connection endpoints must match output and input terminals",
                    );
                }
                index.source_connections.push(edge);
            }
        }
        Ok(index)
    }

    pub fn port(
        &self,
        reference: &BlueprintPortRef,
    ) -> Result<&(BlueprintPort, RotationY), BlueprintError> {
        self.ports
            .get(reference)
            .ok_or_else(|| BlueprintError::Invalid(format!("unknown physical port {reference:?}")))
    }

    pub fn canonical(
        &self,
        reference: &BlueprintPortRef,
    ) -> Result<&BlueprintPortRef, BlueprintError> {
        self.canonical
            .get(reference)
            .ok_or_else(|| BlueprintError::Invalid(format!("unknown physical port {reference:?}")))
    }

    pub fn resolved_port(
        &self,
        reference: &BlueprintPortRef,
    ) -> Result<(BlueprintPort, RotationY), BlueprintError> {
        let canonical = self.canonical(reference)?;
        let (mut port, rotation) = self.port(canonical)?.clone();
        port.required_source_types = self.requirements[canonical].iter().cloned().collect();
        Ok((port, rotation))
    }
}
