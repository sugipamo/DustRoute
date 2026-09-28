//! Unadopted public-workflow input; review and adopt through existing MCP tools.
#[path = "../tests/support/flying_machine_blueprint.rs"]
mod fixture;

fn main() {
    let f = fixture::fixture();
    let mut request = serde_json::to_value(f.request).unwrap();
    request.as_object_mut().unwrap().remove("id");
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "records": {"types": f.catalog.type_revisions().collect::<Vec<_>>(),
                "classifications": f.catalog.classifications().collect::<Vec<_>>(),
                "revisions": f.catalog.revisions().collect::<Vec<_>>(), "assemblies": [f.base]},
            "request": request,
            "finite_flight": {"distance":10,"wait_ticks":160,
                "moving_positions":[{"x":0,"y":0,"z":0},{"x":0,"y":0,"z":1},
                    {"x":0,"y":1,"z":0},{"x":1,"y":0,"z":0},
                    {"x":1,"y":0,"z":1},{"x":1,"y":1,"z":1}]}
        }))
        .unwrap()
    );
}
