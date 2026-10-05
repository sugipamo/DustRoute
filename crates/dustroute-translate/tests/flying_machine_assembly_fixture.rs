//! Unadopted public-workflow input; review and adopt through existing MCP tools.
// The shared fixture also exports harvested-crop cases used by integration tests.
#[allow(dead_code)]
#[path = "../../../tests/support/diagnostic_fixture.rs"]
mod diagnostic_fixture;

#[allow(dead_code)]
#[path = "support/flying_machine_blueprint.rs"]
mod fixture;

#[test]
#[ignore = "explicit offline fixture export; requires new absolute DUSTROUTE_DIAGNOSTIC_OUTPUT"]
fn retain_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = diagnostic_fixture::output()?;
    let f = fixture::fixture();
    let mut request = serde_json::to_value(f.request).unwrap();
    request.as_object_mut().unwrap().remove("id");
    diagnostic_fixture::report(
        &mut output,
        &serde_json::json!({
            "records": {"types": f.catalog.type_revisions().collect::<Vec<_>>(),
                "classifications": f.catalog.classifications().collect::<Vec<_>>(),
                "revisions": f.catalog.revisions().collect::<Vec<_>>(), "assemblies": [f.base]},
            "request": request,
            "finite_flight": {"distance":10,"wait_ticks":160,
                "moving_positions":[{"x":0,"y":0,"z":0},{"x":0,"y":0,"z":1},
                    {"x":0,"y":1,"z":0},{"x":1,"y":0,"z":0},
                    {"x":1,"y":0,"z":1},{"x":1,"y":1,"z":1}]}
        }),
    )?;
    Ok(())
}
