use dustroute_minecraft::device_program::{schema::*, *};
use dustroute_minecraft::{Block, BlockKind};

#[test]
fn rust_laws_preserve_the_entire_published_programs() {
    let frozen = [
        include_str!("fixtures/device-law-v1/lamp-callback-java-1-21-11-v1.json"),
        include_str!("fixtures/device-law-v1/observer-callback-java-1-21-11-v1.json"),
        include_str!("fixtures/device-law-v1/stone-button-callback-java-1-21-11-v1.json"),
    ];
    for (program, json) in dustroute_minecraft::device_callback_law::builtin_programs()
        .iter()
        .zip(frozen)
    {
        let old = serde_json::from_str::<dustroute_minecraft::law::LawProgram>(json).unwrap();
        assert_eq!(*program, old);
    }
}

const VARIANTS: [CheckedDevice; 2] = registry([
    DeviceSpec {
        id: "test.button.a",
        observed_names: &["test_button_a"],
        synthetic: true,
        ..BUILTIN_DEVICES[2].spec()
    }
    .checked(),
    DeviceSpec {
        id: "test.button.b",
        observed_names: &["test_button_b"],
        synthetic: false,
        ..BUILTIN_DEVICES[2].spec()
    }
    .checked(),
]);

#[test]
fn concrete_material_selects_exactly_one_definition_without_a_kind_fallback() {
    let programs = VARIANTS.map(|d| d.compile());
    let mut block = Block::new(BlockKind::Button);
    block.powered = Some(false);
    assert_eq!(
        select(&programs, &block).unwrap().definition().id,
        "test.button.a"
    );
    for (name, id) in [
        ("test_button_a", "test.button.a"),
        ("minecraft:test_button_b", "test.button.b"),
    ] {
        block.observed_name = Some(name.into());
        block
            .observed_properties
            .insert("powered".into(), "false".into());
        assert_eq!(select(&programs, &block).unwrap().definition().id, id);
    }
    assert!(
        programs[0]
            .prepare(Callback::Use, &block, |_| Ok(0))
            .is_err()
    );
    block.observed_name = Some("minecraft:unregistered_button".into());
    assert!(select(&programs, &block).is_none());
    // Production rejects the test-only material, too.
    block.observed_name = Some("minecraft:test_button_a".into());
    assert!(program(&block).is_none());
}

#[test]
fn ambiguous_unchecked_slices_and_unsupported_names_fail_closed() {
    let duplicated = [VARIANTS[0].compile(), VARIANTS[0].compile()];
    let block = Block::new(BlockKind::Button);
    assert!(select(&duplicated, &block).is_none());
    let mut block = Block::new(BlockKind::Observer);
    block.observed_name = Some("another_mod:observer".into());
    assert!(program(&block).is_none());
}
