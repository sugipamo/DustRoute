use crate::{
    Catalog, Compatibility, Component, ComponentId, ComponentKind, Evidence, EvidenceKind,
    LogicalSpec, PhysicalMetrics, Port, PortDirection, Provenance,
};

pub const REDSTONE_COMPILER_XOR_ID: &str = "redstone-compiler.xor-generated";
pub const DUSTROUTE_COMPILED_XOR_ID: &str = "dustroute.xor.compiled-baseline.1-21-11";
pub const DUSTROUTE_COMPACT_XOR_ID: &str = "dustroute.xor.compact-compiled.1-21-11";

#[must_use]
pub fn builtin_catalog() -> Catalog {
    let mut catalog = Catalog::default();
    for component in [
        component(ComponentKind::Not, "not", &["a"], &["out"], |a, _| vec![!a]),
        component(ComponentKind::And, "and", &["a", "b"], &["out"], |a, b| {
            vec![a && b]
        }),
        component(ComponentKind::Or, "or", &["a", "b"], &["out"], |a, b| {
            vec![a || b]
        }),
        component(ComponentKind::Xor, "xor", &["a", "b"], &["out"], |a, b| {
            vec![a ^ b]
        }),
        component(
            ComponentKind::HalfAdder,
            "half-adder",
            &["a", "b"],
            &["sum", "carry"],
            |a, b| vec![a ^ b, a && b],
        ),
        redstone_compiler_xor(),
        dustroute_compiled_xor(),
        dustroute_compact_xor(),
    ] {
        catalog
            .insert(component)
            .expect("built-in component is valid");
    }
    catalog
}

fn component(
    kind: ComponentKind,
    id: &str,
    inputs: &[&str],
    outputs: &[&str],
    evaluate: fn(bool, bool) -> Vec<bool>,
) -> Component {
    let ports = inputs
        .iter()
        .map(|name| Port {
            name: (*name).into(),
            direction: PortDirection::Input,
            bit_width: 1,
        })
        .chain(outputs.iter().map(|name| Port {
            name: (*name).into(),
            direction: PortDirection::Output,
            bit_width: 1,
        }))
        .collect();
    let truth_table = (0..(1_usize << inputs.len()))
        .map(|bits| {
            let a = bits & 1 != 0;
            let b = bits & 2 != 0;
            let mut row = if inputs.len() == 1 {
                vec![a]
            } else {
                vec![a, b]
            };
            row.extend(evaluate(a, b));
            row
        })
        .collect();
    Component {
        id: ComponentId::new(id).expect("static component id is valid"),
        name: id.replace('-', " "),
        kind,
        logical: LogicalSpec {
            ports,
            truth_table,
            stateful: false,
        },
        layout_reference: None,
        physical: None,
        compatibility: None,
        provenance: Provenance {
            author: "DustRoute contributors".into(),
            source_url: None,
            license: Some("Apache-2.0".into()),
            retrieved_on: None,
        },
        evidence: vec![Evidence {
            kind: EvidenceKind::LogicalExhaustive,
            reference: "dustroute-library built-in truth table".into(),
            notes: None,
        }],
    }
}

fn redstone_compiler_xor() -> Component {
    let mut component = component(
        ComponentKind::Xor,
        REDSTONE_COMPILER_XOR_ID,
        &["a", "b"],
        &["out"],
        |a, b| vec![a ^ b],
    );
    component.name = "Redstone Compiler generated XOR".into();
    component.layout_reference = Some("dustroute-translate:external_xor_cell".into());
    component.physical = Some(PhysicalMetrics {
        bounding_size: [3, 5, 5],
        occupied_blocks: 23,
        dust_blocks: 4,
        repeater_count: 0,
        delay_redstone_ticks: None,
    });
    component.compatibility = Some(Compatibility {
        edition: "java".into(),
        versions: Vec::new(),
        incompatible_versions: vec!["1.21.11".into()],
        orientation_sensitive: true,
        update_order_sensitive: true,
    });
    component.provenance = Provenance {
        author: "Redstone-Compiler contributors".into(),
        source_url: Some("https://github.com/Redstone-Compiler/redstone-compiler/blob/cc997732b82d957a8b5cc80d14c07b375562dd9d/test/xor-generated.nbt".into()),
        license: Some("MIT".into()),
        retrieved_on: Some("2026-08-31".into()),
    };
    component.evidence = vec![
        Evidence {
            kind: EvidenceKind::PublishedClaim,
            reference: "upstream xor-generated.nbt at cc997732b82d957a8b5cc80d14c07b375562dd9d"
                .into(),
            notes: Some(
                "Converted from the upstream structure; input levers remain external drivers."
                    .into(),
            ),
        },
        Evidence {
            kind: EvidenceKind::LogicalExhaustive,
            reference: "dustroute-library exhaustive XOR truth table".into(),
            notes: None,
        },
        Evidence {
            kind: EvidenceKind::MinecraftE2eRejected,
            reference: "crates/dustroute-mcp/mineflayer/e2e/scenarios/23-library-xor.json".into(),
            notes: Some("Rejected on Minecraft Java 1.21.11: the published output remains low for all four input combinations in the reconstructed NBT layout.".into()),
        },
    ];
    component
}

fn dustroute_compiled_xor() -> Component {
    let mut component = component(
        ComponentKind::Xor,
        DUSTROUTE_COMPILED_XOR_ID,
        &["a", "b"],
        &["out"],
        |a, b| vec![a ^ b],
    );
    component.name = "DustRoute compiled XOR baseline".into();
    component.layout_reference = Some("dustroute-translate:compiled_xor_cell".into());
    component.physical = Some(PhysicalMetrics {
        bounding_size: [51, 5, 13],
        occupied_blocks: 341,
        dust_blocks: 136,
        repeater_count: 19,
        delay_redstone_ticks: None,
    });
    component.compatibility = Some(Compatibility {
        edition: "java".into(),
        versions: vec!["1.21.11".into()],
        incompatible_versions: Vec::new(),
        orientation_sensitive: true,
        update_order_sensitive: true,
    });
    component.evidence.extend([
        Evidence {
            kind: EvidenceKind::Simulated,
            reference: "dustroute-translate:compiled_baseline_xor_has_the_xor_truth_table".into(),
            notes: Some(
                "All four input combinations settle to XOR after 64 redstone ticks.".into(),
            ),
        },
        Evidence {
            kind: EvidenceKind::MinecraftE2e,
            reference: "crates/dustroute-mcp/mineflayer/e2e/scenarios/26-compiled-xor.json".into(),
            notes: Some(
                "All four stable input combinations passed on Minecraft Java 1.21.11. Observed output transitions settle at 5, 8, or 9 redstone ticks; changing 10 to 01 produces an intermediate low pulse from ticks 3 through 7.".into(),
            ),
        },
    ]);
    component
}

fn dustroute_compact_xor() -> Component {
    let mut component = component(
        ComponentKind::Xor,
        DUSTROUTE_COMPACT_XOR_ID,
        &["a", "b"],
        &["out"],
        |a, b| vec![a ^ b],
    );
    component.name = "DustRoute compact compiled XOR".into();
    component.layout_reference = Some("dustroute-translate:compact_compiled_xor_cell".into());
    component.physical = Some(PhysicalMetrics {
        bounding_size: [39, 5, 11],
        occupied_blocks: 275,
        dust_blocks: 108,
        repeater_count: 14,
        delay_redstone_ticks: None,
    });
    component.compatibility = Some(Compatibility {
        edition: "java".into(),
        versions: vec!["1.21.11".into()],
        incompatible_versions: Vec::new(),
        orientation_sensitive: true,
        update_order_sensitive: true,
    });
    component.evidence.extend([
        Evidence {
            kind: EvidenceKind::Simulated,
            reference: "dustroute-translate:compact_compiled_xor_has_the_xor_truth_table".into(),
            notes: Some("All four input combinations settle to XOR after 64 redstone ticks.".into()),
        },
        Evidence {
            kind: EvidenceKind::MinecraftE2e,
            reference: "crates/dustroute-mcp/mineflayer/e2e/scenarios/27-compact-compiled-xor.json".into(),
            notes: Some("All stable inputs passed on Minecraft Java 1.21.11. Output settles in 5--7 redstone ticks; 10 to 01 has an intermediate low pulse from ticks 3 through 5.".into()),
        },
    ]);
    component
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ComponentQuery, VerificationLevel, verify_logical_spec};

    #[test]
    fn every_builtin_has_an_exhaustively_verified_truth_table() {
        let catalog = builtin_catalog();
        assert_eq!(catalog.len(), 8);
        for kind in [
            ComponentKind::Not,
            ComponentKind::And,
            ComponentKind::Or,
            ComponentKind::Xor,
            ComponentKind::HalfAdder,
        ] {
            let found = catalog.search(&ComponentQuery {
                kind: Some(kind),
                minimum_verification: Some(VerificationLevel::LogicallyVerified),
                ..ComponentQuery::default()
            });
            assert!(!found.is_empty());
            assert!(found.iter().all(|component| {
                verify_logical_spec(kind, &component.logical)
                    .unwrap()
                    .exhaustive
            }));
        }
    }

    #[test]
    fn rejected_external_layout_is_not_an_automatic_replacement() {
        let catalog = builtin_catalog();
        let found = catalog.search(&ComponentQuery {
            require_automatic_replacement: true,
            ..ComponentQuery::default()
        });
        assert_eq!(found.len(), 2);
        assert!(
            found
                .iter()
                .all(|component| component.id.as_str() != REDSTONE_COMPILER_XOR_ID)
        );
        let external = catalog
            .get(&ComponentId::new(REDSTONE_COMPILER_XOR_ID).unwrap())
            .unwrap();
        assert_eq!(
            external.verification_level(),
            VerificationLevel::LogicallyVerified
        );
        assert!(!external.may_automatically_replace_physical_circuit());
    }

    #[test]
    fn minecraft_verified_compiled_xor_is_an_automatic_replacement() {
        let catalog = builtin_catalog();
        let xor = catalog
            .get(&ComponentId::new(DUSTROUTE_COMPILED_XOR_ID).unwrap())
            .unwrap();
        assert_eq!(
            xor.verification_level(),
            VerificationLevel::MinecraftVerified
        );
        assert!(xor.may_automatically_replace_physical_circuit());
    }

    #[test]
    fn minecraft_verified_compact_xor_is_an_automatic_replacement() {
        let catalog = builtin_catalog();
        let xor = catalog
            .get(&ComponentId::new(DUSTROUTE_COMPACT_XOR_ID).unwrap())
            .unwrap();
        assert_eq!(
            xor.verification_level(),
            VerificationLevel::MinecraftVerified
        );
        assert!(xor.may_automatically_replace_physical_circuit());
    }

    #[test]
    fn catalog_component_schema_round_trips_through_json() {
        let catalog = builtin_catalog();
        let components = catalog.search(&ComponentQuery::default());
        let json = serde_json::to_string(&components).unwrap();
        let decoded = Catalog::from_json(&json).unwrap();
        assert_eq!(decoded.len(), catalog.len());
    }
}
