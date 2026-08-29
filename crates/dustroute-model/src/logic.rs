use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Stable identifier for a node in a [`LogicDag`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum GateKind {
    Input,
    Output,
    Not,
    And,
    Or,
    Xor,
    Nand,
}

impl GateKind {
    const fn arity(self) -> usize {
        match self {
            Self::Input => 0,
            Self::Output | Self::Not => 1,
            Self::And | Self::Or | Self::Xor | Self::Nand => 2,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogicNode {
    pub id: NodeId,
    pub kind: GateKind,
    pub inputs: Vec<NodeId>,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogicDag {
    nodes: Vec<LogicNode>,
    outputs: Vec<(String, NodeId)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LogicError {
    DuplicateNode(NodeId),
    MissingNode(NodeId),
    InvalidArity { kind: GateKind, actual: usize },
    Cycle,
    MissingInput(String),
}

impl Display for LogicError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateNode(id) => write!(f, "duplicate node {}", id.0),
            Self::MissingNode(id) => write!(f, "missing node {}", id.0),
            Self::InvalidArity { kind, actual } => {
                write!(f, "invalid arity {actual} for {kind:?}")
            }
            Self::Cycle => f.write_str("logic DAG contains a cycle"),
            Self::MissingInput(name) => write!(f, "missing input {name}"),
        }
    }
}

impl Error for LogicError {}

impl LogicDag {
    pub fn new(nodes: Vec<LogicNode>, outputs: Vec<(String, NodeId)>) -> Result<Self, LogicError> {
        let dag = Self { nodes, outputs };
        dag.validate()?;
        Ok(dag)
    }

    #[must_use]
    pub fn nodes(&self) -> &[LogicNode] {
        &self.nodes
    }

    #[must_use]
    pub fn outputs(&self) -> &[(String, NodeId)] {
        &self.outputs
    }

    pub fn topological_order(&self) -> Result<Vec<NodeId>, LogicError> {
        let mut indegree = BTreeMap::new();
        let mut users: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for node in &self.nodes {
            indegree.insert(node.id, node.inputs.len());
            for input in &node.inputs {
                users.entry(*input).or_default().push(node.id);
            }
        }
        let mut ready: BTreeSet<NodeId> = indegree
            .iter()
            .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
            .collect();
        let mut order = Vec::with_capacity(self.nodes.len());
        while let Some(id) = ready.pop_first() {
            order.push(id);
            if let Some(node_users) = users.get(&id) {
                for user in node_users {
                    let degree = indegree
                        .get_mut(user)
                        .ok_or(LogicError::MissingNode(*user))?;
                    *degree -= 1;
                    if *degree == 0 {
                        ready.insert(*user);
                    }
                }
            }
        }
        if order.len() != self.nodes.len() {
            return Err(LogicError::Cycle);
        }
        Ok(order)
    }

    pub fn logic_depths(&self) -> Result<BTreeMap<NodeId, usize>, LogicError> {
        let by_id: HashMap<_, _> = self.nodes.iter().map(|node| (node.id, node)).collect();
        let mut depths = BTreeMap::new();
        for id in self.topological_order()? {
            let node = by_id[&id];
            let depth = node
                .inputs
                .iter()
                .map(|input| depths[input])
                .max()
                .map_or(0, |value| value + 1);
            depths.insert(id, depth);
        }
        Ok(depths)
    }

    #[must_use]
    pub fn users(&self) -> BTreeMap<NodeId, Vec<NodeId>> {
        let mut users: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
        for node in &self.nodes {
            for input in &node.inputs {
                users.entry(*input).or_default().push(node.id);
            }
        }
        users
    }

    pub fn evaluate(
        &self,
        inputs: &HashMap<String, bool>,
    ) -> Result<BTreeMap<String, bool>, LogicError> {
        let by_id: HashMap<_, _> = self.nodes.iter().map(|node| (node.id, node)).collect();
        let mut values: HashMap<NodeId, bool> = HashMap::new();
        for id in self.topological_order()? {
            let node = by_id[&id];
            let value = match node.kind {
                GateKind::Input => {
                    let name = node.name.as_deref().unwrap_or_default();
                    *inputs
                        .get(name)
                        .ok_or_else(|| LogicError::MissingInput(name.to_owned()))?
                }
                GateKind::Output => values[&node.inputs[0]],
                GateKind::Not => !values[&node.inputs[0]],
                GateKind::And => values[&node.inputs[0]] && values[&node.inputs[1]],
                GateKind::Or => values[&node.inputs[0]] || values[&node.inputs[1]],
                GateKind::Xor => values[&node.inputs[0]] ^ values[&node.inputs[1]],
                GateKind::Nand => !(values[&node.inputs[0]] && values[&node.inputs[1]]),
            };
            values.insert(id, value);
        }
        Ok(self
            .outputs
            .iter()
            .map(|(name, id)| (name.clone(), values[id]))
            .collect())
    }

    pub fn lower_xor(&self) -> Result<Self, LogicError> {
        let by_id: HashMap<_, _> = self.nodes.iter().map(|node| (node.id, node)).collect();
        let mut builder = DagBuilder::new();
        let mut mapped = HashMap::new();
        for id in self.topological_order()? {
            let node = by_id[&id];
            let lowered = if node.kind == GateKind::Input {
                builder.input(node.name.as_deref().unwrap_or_default())
            } else {
                let args: Vec<_> = node.inputs.iter().map(|input| mapped[input]).collect();
                if node.kind == GateKind::Xor {
                    let not_right = builder.gate(GateKind::Not, &[args[1]], None);
                    let not_left = builder.gate(GateKind::Not, &[args[0]], None);
                    let left = builder.gate(GateKind::And, &[args[0], not_right], None);
                    let right = builder.gate(GateKind::And, &[not_left, args[1]], None);
                    builder.gate(GateKind::Or, &[left, right], node.name.as_deref())
                } else {
                    builder.gate(node.kind, &args, node.name.as_deref())
                }
            };
            mapped.insert(id, lowered);
        }
        builder.finish(
            self.outputs
                .iter()
                .map(|(name, id)| (name.clone(), mapped[id])),
        )
    }

    fn validate(&self) -> Result<(), LogicError> {
        let mut ids = BTreeSet::new();
        for node in &self.nodes {
            if !ids.insert(node.id) {
                return Err(LogicError::DuplicateNode(node.id));
            }
            if node.kind.arity() != node.inputs.len() {
                return Err(LogicError::InvalidArity {
                    kind: node.kind,
                    actual: node.inputs.len(),
                });
            }
        }
        for node in &self.nodes {
            for input in &node.inputs {
                if !ids.contains(input) {
                    return Err(LogicError::MissingNode(*input));
                }
            }
        }
        for (_, output) in &self.outputs {
            if !ids.contains(output) {
                return Err(LogicError::MissingNode(*output));
            }
        }
        self.topological_order().map(|_| ())
    }
}

#[derive(Default)]
pub struct DagBuilder {
    nodes: Vec<LogicNode>,
    interned: HashMap<(GateKind, Vec<NodeId>, Option<String>), NodeId>,
}

impl DagBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn input(&mut self, name: &str) -> NodeId {
        self.intern(GateKind::Input, &[], Some(name))
    }

    pub fn gate(&mut self, kind: GateKind, inputs: &[NodeId], name: Option<&str>) -> NodeId {
        assert!(kind != GateKind::Input, "use DagBuilder::input");
        self.intern(kind, inputs, name)
    }

    pub fn finish(
        self,
        outputs: impl IntoIterator<Item = (String, NodeId)>,
    ) -> Result<LogicDag, LogicError> {
        LogicDag::new(self.nodes, outputs.into_iter().collect())
    }

    fn intern(&mut self, kind: GateKind, inputs: &[NodeId], name: Option<&str>) -> NodeId {
        let key = (kind, inputs.to_vec(), name.map(str::to_owned));
        if let Some(id) = self.interned.get(&key) {
            return *id;
        }
        let id = NodeId(u32::try_from(self.nodes.len()).expect("node count exceeds u32"));
        self.nodes.push(LogicNode {
            id,
            kind,
            inputs: inputs.to_vec(),
            name: name.map(str::to_owned),
        });
        self.interned.insert(key, id);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_cycles() {
        let result = LogicDag::new(
            vec![LogicNode {
                id: NodeId(0),
                kind: GateKind::Not,
                inputs: vec![NodeId(0)],
                name: None,
            }],
            vec![("out".into(), NodeId(0))],
        );
        assert_eq!(result.unwrap_err(), LogicError::Cycle);
    }
}
