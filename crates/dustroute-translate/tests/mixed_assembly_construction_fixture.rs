//! Data for the isolated public MCP construction trial; not an adopted record.
#[allow(dead_code)]
#[path = "../../../tests/support/diagnostic_fixture.rs"]
mod diagnostic_fixture;

#[path = "support/runtime_blueprint.rs"]
mod fixture;

#[test]
#[ignore = "explicit offline fixture export; requires new absolute DUSTROUTE_DIAGNOSTIC_OUTPUT"]
fn retain_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = diagnostic_fixture::output()?;
    let fixture = fixture::electrical_fixture(false);
    let mut request = serde_json::to_value(fixture.request).unwrap();
    request.as_object_mut().unwrap().remove("id");
    diagnostic_fixture::report(
        &mut output,
        &serde_json::json!({
            "records":{
                "types":fixture.catalog.type_revisions().collect::<Vec<_>>(),
                "classifications":fixture.catalog.classifications().collect::<Vec<_>>(),
                "revisions":fixture.catalog.revisions().collect::<Vec<_>>(),
                "assemblies":[fixture.base],
            },
            "request":request,
        }),
    )?;
    Ok(())
}
