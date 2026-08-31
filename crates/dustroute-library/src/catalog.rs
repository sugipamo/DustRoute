use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::{Component, ComponentId, ComponentKind, VerificationLevel, verify_logical_spec};

#[derive(Clone, Debug, Default)]
pub struct Catalog {
    components: BTreeMap<ComponentId, Component>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ComponentQuery {
    pub kind: Option<ComponentKind>,
    pub input_count: Option<usize>,
    pub output_count: Option<usize>,
    pub minimum_verification: Option<VerificationLevel>,
    pub require_physical: bool,
    pub require_automatic_replacement: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogError {
    Duplicate(ComponentId),
    Decode(String),
    InvalidLogicalSpec(String),
}

impl Display for CatalogError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Duplicate(id) => write!(f, "duplicate component id {id}"),
            Self::Decode(error) => write!(f, "cannot decode component catalog: {error}"),
            Self::InvalidLogicalSpec(error) => write!(f, "invalid logical specification: {error}"),
        }
    }
}

impl Error for CatalogError {}

impl Catalog {
    pub fn from_json(input: &str) -> Result<Self, CatalogError> {
        let components: Vec<Component> =
            serde_json::from_str(input).map_err(|error| CatalogError::Decode(error.to_string()))?;
        let mut catalog = Self::default();
        for component in components {
            catalog.insert(component)?;
        }
        Ok(catalog)
    }

    pub fn insert(&mut self, component: Component) -> Result<(), CatalogError> {
        verify_logical_spec(component.kind, &component.logical)
            .map_err(|error| CatalogError::InvalidLogicalSpec(error.to_string()))?;
        if self.components.contains_key(&component.id) {
            return Err(CatalogError::Duplicate(component.id));
        }
        self.components.insert(component.id.clone(), component);
        Ok(())
    }

    #[must_use]
    pub fn get(&self, id: &ComponentId) -> Option<&Component> {
        self.components.get(id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.components.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    #[must_use]
    pub fn search(&self, query: &ComponentQuery) -> Vec<&Component> {
        self.components
            .values()
            .filter(|component| query.kind.is_none_or(|kind| component.kind == kind))
            .filter(|component| {
                query
                    .input_count
                    .is_none_or(|count| component.logical.input_names().len() == count)
            })
            .filter(|component| {
                query
                    .output_count
                    .is_none_or(|count| component.logical.output_names().len() == count)
            })
            .filter(|component| {
                query
                    .minimum_verification
                    .is_none_or(|level| component.verification_level() >= level)
            })
            .filter(|component| !query.require_physical || component.physical.is_some())
            .filter(|component| {
                !query.require_automatic_replacement
                    || component.may_automatically_replace_physical_circuit()
            })
            .collect()
    }
}
