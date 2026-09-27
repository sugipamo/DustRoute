//! Data for the isolated public MCP construction trial; not an adopted record.
#[path = "../tests/support/runtime_blueprint.rs"]
mod fixture;

fn main() {
    let fixture = fixture::electrical_fixture(false);
    let mut request = serde_json::to_value(fixture.request).unwrap();
    request.as_object_mut().unwrap().remove("id");
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "records":{
                "types":fixture.catalog.type_revisions().collect::<Vec<_>>(),
                "classifications":fixture.catalog.classifications().collect::<Vec<_>>(),
                "revisions":fixture.catalog.revisions().collect::<Vec<_>>(),
                "assemblies":[fixture.base],
            },
            "request":request,
        }))
        .unwrap()
    );
}
