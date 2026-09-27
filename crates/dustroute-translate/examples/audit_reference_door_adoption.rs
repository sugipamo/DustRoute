//! Read-only model audit. Each command emits JSON; no live world is contacted.
//! `prepare` creates an unadopted proposal. `adopt <archive.json>` revalidates a
//! saved proposal in a separate process; a historical pass is never reused.
#[path = "../tests/support/reference_door_blueprint.rs"]
mod fixture;

use dustroute_translate::assembly_transform::AssemblyTransform;
use dustroute_translate::behavior_type::BehaviorBudget;
use dustroute_translate::blueprint_update::{
    BlueprintUpdateError, BlueprintUpdates, RecordedReview,
};
use dustroute_translate::piston_construction::ElectricalConstruction;
use dustroute_translate::runtime_review::review_assembly_in_runtime_context;
use serde_json::json;

fn live_probes(
    f: &fixture::Fixture,
    target: Option<AssemblyTransform>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    use dustroute_minecraft::time::piston_runtime::{
        new_piston_runtime, schedule_electrical_input_after_tick,
    };
    use dustroute_translate::piston_construction::electrical_snapshot;
    let (world, context) = if let Some(transform) = target {
        let mut catalog = f.catalog.clone();
        catalog.insert_revisions(f.request.revisions.clone())?;
        let (assembly, context) =
            transform.apply(&f.request.candidate_state.assembly, &f.context)?;
        (assembly.inspect(&catalog)?.proposed_world(), context)
    } else {
        (f.world.clone(), f.context.clone())
    };
    let mut run = new_piston_runtime(world, context.known_region, context.root_limits)?;
    run.run_until_idle()?;
    let mut probes = vec![];
    for powered in [true, false, true, false] {
        let tick = run.view().time().game_tick + 1;
        schedule_electrical_input_after_tick(&mut run, tick, context.input_levers[0], powered)?;
        run.run_until_idle()?;
        probes.push(json!({"powered":powered,
            "expected":electrical_snapshot(run.view().world(), context.known_region)?}));
    }
    let source: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/reference-3x3-bobiloosky-v1.json"
    ))?;
    let mut aperture: Vec<dustroute_translate::Pos> =
        serde_json::from_value(source["aperture"].clone())?;
    if let Some(transform) = target {
        for pos in &mut aperture {
            *pos = transform.position(*pos)?;
        }
    }
    let mut result = json!({"aperture":aperture,"snapshots":probes,
        "wait_ticks":100,
        "scope":"finite live trial; client wait and matching snapshots do not certify hidden pending events"});
    if let Some(transform) = target {
        result["target_transform"] = serde_json::to_value(transform)?;
    }
    Ok(result)
}

/// Independent static audit of the candidate order, not a replacement planner.
/// Dynamic placement effects are still evaluated by ElectricalConstruction.
fn target_construction(
    f: &fixture::Fixture,
    transform: AssemblyTransform,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    use dustroute_minecraft::piston_electrical::along;
    use dustroute_translate::{BlockKind, Pos};
    use std::collections::{BTreeMap, BTreeSet};
    let mut catalog = f.catalog.clone();
    catalog.insert_revisions(f.request.revisions.clone())?;
    let (assembly, context) = transform.apply(&f.request.candidate_state.assembly, &f.context)?;
    let world = assembly.inspect(&catalog)?.proposed_world();
    let plan = ElectricalConstruction::new(&world, context.known_region, context.root_limits)?;
    let indices: BTreeMap<_, _> = plan
        .build_steps()
        .iter()
        .enumerate()
        .map(|(i, step)| (step.position, i))
        .collect();
    let mut edges = vec![];
    for (pos, block) in world.iter() {
        if let Some(d) = block.support_offset {
            let support = Pos::new(
                pos.x.checked_add(d.x).ok_or("support overflow")?,
                pos.y.checked_add(d.y).ok_or("support overflow")?,
                pos.z.checked_add(d.z).ok_or("support overflow")?,
            );
            edges.push((support, *pos, "support"));
        }
        if block.kind == BlockKind::Observer {
            let watched = along(
                *pos,
                block.facing.ok_or("observer facing required")?.opposite(),
            )?;
            if world.get(watched).is_some_and(|b| b.kind != BlockKind::Air) {
                edges.push((watched, *pos, "occupied_watched_cell"));
            }
        }
    }
    edges.sort();
    for (before, after, _) in &edges {
        if indices
            .get(before)
            .zip(indices.get(after))
            .is_none_or(|(a, b)| a >= b)
        {
            return Err(
                format!("candidate violates static dependency {before:?} -> {after:?}").into(),
            );
        }
    }
    // Kahn-style selection independently checks whether the static graph is a DAG.
    let mut remaining: BTreeSet<_> = indices.keys().copied().collect();
    let mut topological_order = vec![];
    while !remaining.is_empty() {
        let next = remaining
            .iter()
            .copied()
            .find(|pos| {
                !edges
                    .iter()
                    .any(|(before, after, _)| after == pos && remaining.contains(before))
            })
            .ok_or("static dependency cycle")?;
        remaining.remove(&next);
        topological_order.push(next);
    }
    Ok(
        json!({"transform":transform,"initial":plan.initial(),"settled":plan.settled(),
        "build":plan.build_steps(),"remove":plan.remove_steps(),
        "dependencies":edges.into_iter().map(|(before, after, reason)|
            json!({"before":before,"after":after,"reason":reason})).collect::<Vec<_>>(),
        "static_topological_order":topological_order,
        "scope":"static support/watch constraints plus full production runtime construction; not arbitrary order search"}),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let command = args.next().ok_or(
        "expected prepare, live-prepare, construction, review, adopt or recheck (optional ordinary- prefix)",
    )?;
    let (ordinary, command) = command
        .strip_prefix("ordinary-")
        .map_or((false, command.as_str()), |command| (true, command));
    let f = if ordinary {
        fixture::ordinary_fixture()
    } else {
        fixture::fixture()
    };
    let value = match command {
        "prepare" | "live-prepare" | "target-live-prepare" => {
            let target = if command == "target-live-prepare" {
                Some(serde_json::from_str::<AssemblyTransform>(
                    &std::fs::read_to_string(
                        args.next().ok_or("expected target transform JSON path")?,
                    )?,
                )?)
            } else {
                None
            };
            let target_plan = target.map(|t| target_construction(&f, t)).transpose()?;
            let probes = if command != "prepare" {
                if !ordinary {
                    return Err("live-prepare requires the ordinary contract".into());
                }
                Some(live_probes(&f, target)?)
            } else {
                None
            };
            let mut request = serde_json::to_value(&f.request)?;
            request.as_object_mut().unwrap().remove("id");
            let records = json!({
                "types":f.catalog.type_revisions().collect::<Vec<_>>(),
                "classifications":f.catalog.classifications().collect::<Vec<_>>(),
                "revisions":f.catalog.revisions().collect::<Vec<_>>(),
                "assemblies":[f.base],
            });
            let mut updates = BlueprintUpdates::new(f.catalog);
            updates.create(f.request)?;
            let mut prepared = json!({"records":records,"request":request,
                "archive":serde_json::from_str::<serde_json::Value>(&updates.to_json()?)?});
            if let Some(probes) = probes {
                prepared["live_probes"] = probes;
            }
            if let Some(plan) = target_plan {
                prepared["target_construction"] = plan;
            }
            prepared
        }
        "construction" => match ElectricalConstruction::new(
            &f.world,
            f.context.known_region,
            f.context.root_limits,
        ) {
            Ok(plan) => json!({"status":"passed", "initial":plan.initial(),
                "settled":plan.settled(),"build":plan.build_steps(),"remove":plan.remove_steps()}),
            Err(error) => json!({"status":"failed","error":error}),
        },
        "review" => {
            let mut catalog = f.catalog;
            catalog.insert_revisions(f.request.revisions)?;
            let report = review_assembly_in_runtime_context(
                &catalog,
                &f.request.candidate_state.assembly,
                &f.context,
                BehaviorBudget::default(),
            )?;
            json!({"status":report.status(),"context":report.context,
                "occurrences":report.occurrences.into_iter().collect::<Vec<_>>(),
                "arrangement":report.arrangement,"behavior":report.behavior})
        }
        "adopt" => {
            let input: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
                args.next()
                    .ok_or("expected saved audit JSON containing archive")?,
            )?)?;
            let mut updates =
                BlueprintUpdates::from_json(&serde_json::to_string(&input["archive"])?)?;
            let id = &f.request.id;
            let result = match updates.adopt(id) {
                Ok(()) => json!({"status":"adopted"}),
                Err(BlueprintUpdateError::Validation(report)) => json!({
                    "status":report.status(),"review":RecordedReview::from(report.as_ref())}),
                Err(error) => return Err(error.into()),
            };
            json!({"result":result,"proposal_status":updates.proposal(id).unwrap().status(),
                "candidate_published":updates.catalog().assembly(&f.request.candidate_state.id).is_some(),
                "archive":serde_json::from_str::<serde_json::Value>(&updates.to_json()?)?})
        }
        "recheck" => {
            let input: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
                args.next()
                    .ok_or("expected saved audit JSON containing archive")?,
            )?)?;
            let updates = BlueprintUpdates::from_json(&serde_json::to_string(&input["archive"])?)?;
            let request = updates
                .proposal(&f.request.id)
                .ok_or("saved proposal missing")?
                .request();
            let candidate = updates
                .catalog()
                .assembly(&request.candidate_state.id)
                .ok_or("candidate is not published in saved catalog")?;
            let report = dustroute_translate::promotion::review_assembly_with_context(
                updates.catalog(),
                &candidate.assembly,
                request.behavior_context.as_ref(),
                BehaviorBudget::default(),
            )?;
            json!({"status":report.status(),"review":RecordedReview::from(&report)})
        }
        _ => {
            return Err(
                "expected prepare, live-prepare, construction, review, adopt or recheck".into(),
            );
        }
    };
    if args.next().is_some() {
        return Err("unexpected extra argument".into());
    }
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
