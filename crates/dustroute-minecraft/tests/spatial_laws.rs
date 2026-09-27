use dustroute_minecraft::law::{Expr, Instruction};
use dustroute_minecraft::spatial::{
    SpatialLaws, WeakTarget, builtin_programs, builtin_spatial_laws,
};
use dustroute_minecraft::{Block, BlockKind, BlockRedstoneTraits, WireConnection};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct PriorTraits {
    kind: BlockKind,
    name: Option<String>,
    state: BTreeMap<String, String>,
    properties: [bool; 5],
    traits: BlockRedstoneTraits,
}

#[test]
fn traits_match_the_pre_migration_kernel_for_all_kinds_and_observed_forms() {
    // Captured before replacing the Rust rules, including unsupported and
    // inconsistent kind/name combinations so adapter precedence stays fixed.
    let rows = include_str!("fixtures/spatial_traits_v1.jsonl")
        .lines()
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 144);
    for row in rows {
        let prior: PriorTraits = serde_json::from_str(row).unwrap();
        let mut block = Block::new(prior.kind);
        block.observed_name = prior.name;
        block.observed_properties = prior.state;
        assert_eq!(block.redstone_traits(), prior.traits, "{block:?}");
        let p = prior.kind.properties();
        assert_eq!(
            [
                p.supports_components,
                p.receives_weak_power,
                p.receives_strong_power,
                p.repeater_reads_block_power,
                p.strong_power_drives_dust
            ],
            prior.properties
        );
    }
}

#[test]
fn changed_programs_change_physical_queries_without_changing_the_pinned_default() {
    let original = builtin_programs().clone();
    let mut changed = original.clone();
    changed[0]
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::Set {
            register: "dust_support".into(),
            value: Expr::Constant { value: 0 },
        });
    changed[3].handlers.insert(
        "evaluate".into(),
        vec![Instruction::Set {
            register: "connected".into(),
            value: Expr::Constant { value: 0 },
        }],
    );
    changed[0]
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::Set {
            register: "strong_targets".into(),
            value: Expr::Constant { value: 1 },
        });
    let laws = SpatialLaws::compile(&changed).unwrap();
    let solid = Block::new(BlockKind::Solid);
    assert!(!laws.block_traits(&solid).supports_dust_on_top);
    assert!(!laws.weakly_powers(WeakTarget::Horizontal, WireConnection::Side));
    assert!(solid.redstone_traits().supports_dust_on_top);
    assert!(builtin_spatial_laws().weakly_powers(WeakTarget::Horizontal, WireConnection::Side));
    assert_eq!(*builtin_programs(), original);
    let mut world = dustroute_minecraft::World::new();
    let support = dustroute_minecraft::Pos::default();
    let torch = dustroute_minecraft::Pos::new(1, 0, 0);
    world.set(support, solid);
    world.place(BlockKind::RedstoneWire, support.offset(0, 1, 0));
    world.place(BlockKind::RedstoneTorch, torch).support_offset =
        Some(dustroute_minecraft::Pos::new(-1, 0, 0));
    assert!(world.support_issues().is_empty());
    assert_eq!(world.support_issues_with_laws(&laws).len(), 2);
    assert_eq!(
        laws.strong_output_targets(&world, torch),
        Some(vec![support])
    );
    assert_eq!(
        builtin_spatial_laws().strong_output_targets(&world, torch),
        Some(vec![torch.offset(0, 1, 0)])
    );
}

#[test]
fn spatial_adapter_rejects_wrong_abis_events_and_out_of_bounds_results() {
    let mut programs = builtin_programs().clone();
    programs[3].inputs.insert("unprovided_fact".into(), 1);
    assert!(SpatialLaws::compile(&programs).is_err());
    let mut programs = builtin_programs().clone();
    programs[3]
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::ScheduleIfAbsent {
            event: "evaluate".into(),
            after: 1,
        });
    assert!(SpatialLaws::compile(&programs).is_err());
    let mut programs = builtin_programs().clone();
    programs[0]
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::Set {
            register: "shape".into(),
            value: Expr::Constant { value: 5 },
        });
    assert!(SpatialLaws::compile(&programs).is_err());
}

#[test]
fn finite_shape_transfer_and_weak_power_domains_preserve_the_existing_predicates() {
    use WireConnection::{None as NoArm, Side, Up};
    use dustroute_minecraft::spatial::{DustElevation, DustTransferFacts, WireShapeFacts};
    let law = builtin_spatial_laws();
    // Reference predicates are the pre-migration wire.rs/redstone.rs rules,
    // including the explicitly retained difference in rise-clearance scope.
    for support in [None, Some(Side), Some(Up)] {
        for bits in 0..32 {
            let facts = WireShapeFacts {
                side_connects: bits & 1 != 0,
                support_shape: support,
                upper_wire: bits & 2 != 0,
                blocked: bits & 4 != 0,
                side_air: bits & 8 != 0,
                lower_wire: bits & 16 != 0,
            };
            let expected = if facts.side_connects {
                Side
            } else if let Some(shape) = support.filter(|_| facts.upper_wire && !facts.blocked) {
                shape
            } else if facts.side_air && facts.lower_wire {
                Side
            } else {
                NoArm
            };
            assert_eq!(law.infer_shape(facts), expected, "{facts:?}");
        }
        for source_arm in [NoArm, Side, Up] {
            for sink_arm in [NoArm, Side, Up] {
                for bits in 0..8 {
                    for elevation in [
                        DustElevation::Horizontal,
                        DustElevation::Rise,
                        DustElevation::Fall,
                    ] {
                        let facts = DustTransferFacts {
                            elevation,
                            source_arm,
                            sink_arm,
                            support_shape: support,
                            conducts: bits & 1 != 0,
                            clear: bits & 2 != 0,
                            enforce_clearance: bits & 4 != 0,
                        };
                        let expected = match elevation {
                            DustElevation::Horizontal => source_arm != NoArm && sink_arm != NoArm,
                            DustElevation::Rise => {
                                support == Some(source_arm)
                                    && (!facts.enforce_clearance || facts.clear)
                            }
                            DustElevation::Fall => support == Some(sink_arm) && facts.conducts,
                        };
                        assert_eq!(law.dust_transfers(facts), expected, "{facts:?}");
                    }
                }
            }
        }
    }
    for (relation, expected) in [
        (WeakTarget::Below, [true, true, true]),
        (WeakTarget::Horizontal, [false, true, true]),
        (WeakTarget::Other, [false, false, false]),
    ] {
        for (arm, connected) in [NoArm, Side, Up].into_iter().zip(expected) {
            assert_eq!(law.weakly_powers(relation, arm), connected);
        }
    }
}
