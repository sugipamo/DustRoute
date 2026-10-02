//! Authored reference roof: selection/preflight is detached; execution uses the
//! production common executor. No test-private native physics/action entrypoint.
use super::*;
use crate::survival_execution::{ExecutionProgress, SurvivalExecutor};
use serde_json::{Value, json};
use std::{io::Write, time::Duration};
use voxrig::checked_survival::{InventorySlot, SurvivalInput, SurvivalScenario};
use voxrig::{Client, ConnectionConfig, MinecraftVersion, Server};

const FLOOR: i32 = -60;
fn p(x: i32, y: i32, z: i32) -> [i32; 3] {
    [x, FLOOR + y, z]
}
fn r(a: [i32; 3], b: [i32; 3]) -> Region {
    Region::new(pos(a), pos(b))
}
fn config(name: &str) -> ConnectionConfig {
    ConnectionConfig::offline(
        Server::new("127.0.0.1", 25572),
        name,
        MinecraftVersion::Java1_21_11,
    )
}
fn rotation(eye: [f64; 3], target: [f64; 3]) -> [f32; 2] {
    let d: [f64; 3] = std::array::from_fn(|i| target[i] - eye[i]);
    [
        (-d[0]).atan2(d[2]).to_degrees() as f32,
        (-d[1]).atan2(d[0].hypot(d[2])).to_degrees() as f32,
    ]
}
fn face_delta(face: &BlockFace) -> [i32; 3] {
    match face {
        BlockFace::Up => [0, 1, 0],
        BlockFace::Down => [0, -1, 0],
        BlockFace::North => [0, 0, -1],
        BlockFace::South => [0, 0, 1],
        BlockFace::West => [-1, 0, 0],
        BlockFace::East => [1, 0, 0],
    }
}
fn scope() -> ConstructionScope {
    ConstructionScope {
        observed: r(p(-5, -2, -5), p(9, 10, 10)),
        edits: WorldEditScope {
            editable: vec![r(p(-4, 0, -4), p(8, 8, 9))],
            protected: vec![r(p(-5, -1, -5), p(9, -1, 10))],
        },
        temporary: vec![
            r(p(1, 0, 6), p(2, 3, 6)),
            r(p(1, 0, -2), p(2, 5, -2)),
            r(p(1, 5, -1), p(2, 5, -1)),
        ],
        travel: TravelBounds {
            min: [-4.0, f64::from(FLOOR), -4.0],
            max: [8.0, f64::from(FLOOR) + 10.0, 9.0],
        },
        retreat: TravelBounds {
            min: [0.5, f64::from(FLOOR), -3.5],
            max: [3.5, f64::from(FLOOR), -1.1],
        },
    }
}
fn design() -> std::result::Result<GeneratedGroundedBuildingDesign, String> {
    let region = |a, b| json!(r(a, b));
    dustroute_translate::building::generate_grounded_building_design(serde_json::from_value(json!({
        "ground_material":"stone","design":{
            "namespace":"test.survival-roof","name":"Fixed elevated roof",
            "known_region":region(p(-1,-1,-1),p(5,7,5)),
            "parts":[{"name":"roof","shapes":[{"kind":"fill","material":"cobblestone","region":region(p(0,6,0),p(4,6,4))}]},
                {"name":"columns","shapes":[
                    {"kind":"fill","material":"cobblestone","region":region(p(0,0,0),p(0,5,0))},
                    {"kind":"fill","material":"cobblestone","region":region(p(4,0,0),p(4,5,0))},
                    {"kind":"fill","material":"cobblestone","region":region(p(0,0,4),p(0,5,4))},
                    {"kind":"fill","material":"cobblestone","region":region(p(4,0,4),p(4,5,4))}]}],
            "spaces":[{"name":"inside","region":region(p(1,0,1),p(3,5,3))}]
        }
    })).map_err(|e| format!("roof specification: {e}"))?).map_err(|e| format!("roof Blueprint: {e:?}"))
}
struct Recipe<'a> {
    scenario: SurvivalScenario,
    actions: Vec<ConstructionAction>,
    evidence: &'a mut Vec<Value>,
}
impl Recipe<'_> {
    fn eye(&self) -> [f64; 3] {
        let mut eye = self.scenario.position();
        eye[1] += f64::from(1.62f32);
        eye
    }
    fn put(
        &mut self,
        target: [i32; 3],
        face: BlockFace,
        temporary: bool,
    ) -> std::result::Result<(), String> {
        let delta = face_delta(&face);
        let support = std::array::from_fn(|i| target[i] - delta[i]);
        let point =
            std::array::from_fn(|i| f64::from(support[i]) + 0.5 + f64::from(delta[i]) * 0.5);
        let aim = rotation(self.eye(), point);
        let material = if temporary { "dirt" } else { "cobblestone" };
        let placement = self
            .scenario
            .preview_cube_placement(support, face, aim, material)
            .map_err(|e| {
                format!(
                    "place {target:?} via {support:?}/{face:?} from {:?}: {e}",
                    self.scenario.position()
                )
            })?;
        if placement.edit.position != target {
            return Err("unexpected hypothetical target".into());
        }
        self.scenario = self
            .scenario
            .after_edits(&[placement.edit])
            .map_err(|e| e.to_string())?;
        self.actions.push(ConstructionAction::Place {
            purpose: if temporary {
                PlacementPurpose::Temporary
            } else {
                PlacementPurpose::Permanent
            },
            support,
            face,
            rotation: aim,
            material: material.into(),
        });
        self.evidence
            .push(json!({"phase":"planned_place","target":target,"temporary":temporary}));
        Ok(())
    }
    fn remove(&mut self, target: [i32; 3]) -> std::result::Result<(), String> {
        let mut errors = Vec::new();
        for face in [
            BlockFace::Up,
            BlockFace::North,
            BlockFace::South,
            BlockFace::West,
            BlockFace::East,
        ] {
            let delta = face_delta(&face);
            let point =
                std::array::from_fn(|i| f64::from(target[i]) + 0.5 + f64::from(delta[i]) * 0.5);
            let aim = rotation(self.eye(), point);
            match self.scenario.preview_cube_removal(target, face, aim) {
                Ok(edit) => {
                    self.scenario = self
                        .scenario
                        .after_edits(&[edit])
                        .and_then(|next| next.after_expected_reconnect().map(|(next, _)| next))
                        .map_err(|e| e.to_string())?;
                    self.actions.push(ConstructionAction::RemoveTemporary {
                        target,
                        face,
                        rotation: aim,
                    });
                    self.evidence
                        .push(json!({"phase":"planned_remove","target":target}));
                    return Ok(());
                }
                Err(e) => errors.push(e.to_string()),
            }
        }
        Err(format!(
            "remove {target:?} from {:?}: {errors:?}",
            self.scenario.position()
        ))
    }
    fn go(
        &mut self,
        target: [f64; 3],
        tolerance: f64,
        jump: bool,
    ) -> std::result::Result<(), String> {
        let from = self.scenario.position();
        let yaw = rotation(from, target)[0];
        let mut best = None;
        let mut nearest = None;
        let mut refusals = BTreeMap::new();
        for active in 1..=48 {
            let controls: Vec<_> = (0..active + 24)
                .map(|t| SurvivalControl {
                    yaw,
                    input: SurvivalInput {
                        forward: i8::from(t < active),
                        strafe: 0,
                        jump: jump && t == 0,
                    },
                })
                .collect();
            match self.scenario.preview_path(&controls) {
                Ok(preview) => {
                    if !survival_navigation::hypothetical_admissible(&preview, scope().travel) {
                        continue;
                    }
                    let end = preview.frames.last().unwrap().position;
                    let error = (end[0] - target[0]).hypot(end[2] - target[2]);
                    if (end[1] - target[1]).abs() < 0.01 {
                        if nearest.as_ref().is_none_or(|(score, _, _)| error < *score) {
                            nearest = Some((error, active, end));
                        }
                        if error <= tolerance
                            && best.as_ref().is_none_or(|(score, _)| error < *score)
                        {
                            best = Some((error, controls));
                        }
                    }
                }
                Err(e) => {
                    *refusals.entry(e.to_string()).or_insert(0usize) += 1;
                }
            }
        }
        let Some((_, controls)) = best else {
            self.evidence.push(json!({"phase":"waypoint_refused","from":from,"target":target,"jump":jump,"nearest":nearest,"refusals":refusals}));
            return Err(format!(
                "no bounded waypoint candidate {from:?}->{target:?} jump={jump}; nearest={nearest:?}"
            ));
        };
        self.scenario = self
            .scenario
            .after_path(&controls)
            .map_err(|e| e.to_string())?;
        self.evidence.push(json!({"phase":"planned_move","from":from,"target":target,"end":self.scenario.position(),"ticks":controls.len(),"jump":jump}));
        self.actions.push(ConstructionAction::Move { controls });
        Ok(())
    }
    fn tower(&mut self, z: i32, column_z: i32, height: i32) -> std::result::Result<i32, String> {
        let mut current = 2;
        self.put(p(current, 0, z), BlockFace::Up, true)?;
        self.go(
            [
                f64::from(current) + 0.5,
                f64::from(FLOOR + 1),
                f64::from(z) + 0.5,
            ],
            0.13,
            true,
        )?;
        for y in 0..=2 {
            for x in [0, 4] {
                self.put(p(x, y, column_z), BlockFace::Up, false)?;
            }
        }
        for h in 2..=height {
            let next = 3 - current;
            for y in h - 2..h {
                self.put(p(next, y, z), BlockFace::Up, true)?;
            }
            self.go(
                [
                    f64::from(next) + 0.5,
                    f64::from(FLOOR + h),
                    f64::from(z) + 0.5,
                ],
                0.13,
                true,
            )?;
            current = next;
            if h <= 4 {
                for x in [0, 4] {
                    self.put(p(x, h + 1, column_z), BlockFace::Up, false)?;
                }
            }
        }
        Ok(current)
    }
    fn descend(
        &mut self,
        z: i32,
        mut current: i32,
        height: i32,
    ) -> std::result::Result<(), String> {
        for h in (1..=height).rev() {
            let next = 3 - current;
            self.go(
                [
                    f64::from(next) + 0.5,
                    f64::from(FLOOR + h - 1),
                    f64::from(z) + 0.5,
                ],
                // Whole-tick input can land off-center after a received reset.
                // Native terminal clearance and travel bounds still admit it.
                0.23,
                false,
            )?;
            for y in ((h - 2).max(0)..h).rev() {
                self.remove(p(current, y, z))?;
            }
            current = next;
        }
        Ok(())
    }
}
fn plan(
    scene: &CapturedSurvivalScene,
    evidence: &mut Vec<Value>,
) -> std::result::Result<HypotheticalConstructionPlan, String> {
    let site = ConstructionSite::from_grounded(&design()?, scope()).map_err(|e| e.to_string())?;
    let mut recipe = Recipe {
        scenario: scene.scenario(),
        actions: Vec::new(),
        evidence,
    };
    let top = recipe.tower(6, 4, 4)?;
    recipe.descend(6, top, 4)?;
    recipe.go([2.5, f64::from(FLOOR), -2.5], 0.13, false)?;
    let top = recipe.tower(-2, 0, 6)?;
    for x in [0, 4] {
        recipe.put(p(x, 6, 0), BlockFace::Up, false)?;
    }
    for (x, face) in [
        (1, BlockFace::East),
        (3, BlockFace::West),
        (2, BlockFace::West),
    ] {
        recipe.put(p(x, 6, 0), face, false)?;
    }
    let bridge_x = top;
    recipe.go(
        [f64::from(top) + 0.5, f64::from(FLOOR + 6), -0.88],
        0.1,
        false,
    )?;
    recipe.put(p(bridge_x, 5, -1), BlockFace::South, true)?;
    recipe.go(
        [f64::from(top) + 0.5, f64::from(FLOOR + 6), -0.5],
        0.13,
        false,
    )?;
    recipe.go(
        [f64::from(top) + 0.5, f64::from(FLOOR + 7), 0.5],
        0.13,
        true,
    )?;
    recipe.go([2.5, f64::from(FLOOR + 7), 0.5], 0.13, false)?;
    for z in 1..=4 {
        recipe.go(
            [2.5, f64::from(FLOOR + 7), f64::from(z) + 0.12],
            0.10,
            false,
        )?;
        for x in [0, 4, 1, 3, 2] {
            recipe.put(p(x, 6, z), BlockFace::South, false)?;
        }
    }
    recipe.go(
        [f64::from(top) + 0.5, f64::from(FLOOR + 7), 0.5],
        0.13,
        false,
    )?;
    recipe.go(
        [f64::from(top) + 0.5, f64::from(FLOOR + 6), -0.5],
        0.13,
        false,
    )?;
    recipe.go(
        [f64::from(top) + 0.5, f64::from(FLOOR + 6), -1.5],
        0.13,
        false,
    )?;
    recipe.remove(p(bridge_x, 5, -1))?;
    recipe.descend(-2, top, 6)?;
    let result = preview_construction_sequence(
        scene,
        &site,
        &recipe.actions,
        &BTreeMap::from([
            ("minecraft:cobblestone".into(), 49),
            ("minecraft:dirt".into(), 32),
        ]),
    )
    .map_err(|e| e.to_string())?;
    recipe
        .evidence
        .push(json!({"phase":"checked_complete_plan","plan":result}));
    Ok(result)
}

#[tokio::test]
#[ignore = "explicit isolated non-OP roof fixture; preflight is default, live needs opt-in"]
async fn native_fixed_roof() {
    assert_eq!(
        std::env::var("DUSTROUTE_SURVIVAL_ROOF_PORT").unwrap(),
        "25572"
    );
    let output = std::env::var("DUSTROUTE_SURVIVAL_ROOF_OUTPUT").unwrap();
    let file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&output)
        .unwrap();
    let mut bot = Client::connect(config("NatMineBot")).await.unwrap();
    bot.wait_until_ready().await.unwrap();
    let viewer = Client::connect(config("NatMineView")).await.unwrap();
    viewer.wait_until_ready().await.unwrap();
    println!(
        "FIXTURE roof: flat stone y=-61; air above; bot 2.5 -60 7.5; viewer 7.5 -60 2.5; clear bot; inventory.0 cobblestone 49, inventory.1 dirt 32; enter"
    );
    std::io::stdout().flush().unwrap();
    tokio::time::timeout(
        Duration::from_secs(60),
        tokio::task::spawn_blocking(|| {
            let mut s = String::new();
            std::io::stdin().read_line(&mut s)
        }),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    let ops = bot.survival().unwrap();
    tokio::time::timeout(Duration::from_secs(5),async {loop {
        let player=ops.player_state().await.unwrap();
        if matches!(&player.inventory.slots[9],InventorySlot::Item{item} if item.name=="minecraft:cobblestone" && item.count==49)
            && matches!(&player.inventory.slots[10],InventorySlot::Item{item} if item.name=="minecraft:dirt" && item.count==32)
            && ops.standing_context().await.is_ok_and(|s|s.position==[2.5,f64::from(FLOOR),7.5]) {break;}
        tokio::time::sleep(Duration::from_millis(20)).await;
    }}).await.unwrap();
    bot.start_packet_trace(8_388_608).await.unwrap();
    viewer.start_packet_trace(8_388_608).await.unwrap();
    let before = ops.player_state().await.unwrap();
    let mut events = Vec::new();
    let mut traces = Vec::new();
    let live = std::env::var("DUSTROUTE_SURVIVAL_ROOF_EXECUTE").as_deref() == Ok("1");
    let result = exercise(&mut bot, &viewer, &output, live, &mut events, &mut traces).await;
    let history = bot.survival().unwrap().operation_history().await;
    if let Ok(t) = bot.stop_packet_trace().await {
        traces.push(json!(t));
    }
    let observer_trace = viewer.stop_packet_trace().await.unwrap();
    let disconnect = bot.disconnect().await.map_err(|e| e.to_string());
    viewer.disconnect().await.unwrap();
    serde_json::to_writer(file,&json!({"scope":"fixed reference roof; shared checked geometry/executor; public MCP adoption is subsequent","execute":live,"before":before,"events":events,"history":history,"traces":traces,"observer_trace":observer_trace,"error":result.as_ref().err(),"disconnect_error":disconnect.err()})).unwrap();
    result.unwrap();
}
async fn exercise(
    bot: &mut Client,
    viewer: &Client,
    output: &str,
    live: bool,
    events: &mut Vec<Value>,
    traces: &mut Vec<Value>,
) -> std::result::Result<(), String> {
    let observed = scope().observed;
    let scene = bot
        .survival()
        .map_err(|e| e.to_string())?
        .capture_survival_scene(voxrig::Region {
            min: xyz(observed.min),
            max: xyz(observed.max),
        })
        .await
        .map_err(|e| e.to_string())?;
    let plan = if std::env::var("DUSTROUTE_SURVIVAL_ROOF_PLANNER").as_deref() == Ok("generated") {
        let site =
            ConstructionSite::from_grounded(&design()?, scope()).map_err(|e| e.to_string())?;
        let mut limits = generation::SearchLimits::default();
        if let Ok(value) = std::env::var("DUSTROUTE_SURVIVAL_ROOF_CHECKS") {
            limits.candidate_checks = value.parse().map_err(|e| format!("search budget: {e}"))?;
        }
        let result = generation::generate_construction_plan(
            &scene,
            &site,
            &BTreeMap::from([
                ("minecraft:cobblestone".into(), 49),
                ("minecraft:dirt".into(), 32),
            ]),
            "minecraft:dirt",
            limits,
        );
        events.push(json!({"phase":"generated_plan_result","result":result,"limits":limits}));
        result
            .map_err(|e| format!("automatic construction: {e:?}"))?
            .plan
    } else {
        plan(&scene, events)?
    };
    if !live {
        return Ok(());
    }
    let directory = std::path::PathBuf::from(format!("{output}.journal"));
    let mut executor = SurvivalExecutor::create(
        bot.clone(),
        viewer.clone(),
        config("NatMineBot"),
        plan,
        &directory,
    )
    .await
    .map_err(|e| e.to_string())?;
    let mut reconnects = 0;
    loop {
        let result = executor.advance().await;
        if executor.record().reconnects != reconnects {
            if let Ok(t) = bot.stop_packet_trace().await {
                traces.push(json!(t));
            }
            *bot = executor.client().clone();
            bot.start_packet_trace(8_388_608)
                .await
                .map_err(|e| e.to_string())?;
            reconnects = executor.record().reconnects;
        }
        match result {
            Ok(progress) => {
                println!(
                    "PROGRESS {} {:?}",
                    executor.record().completed_steps,
                    progress
                );
                events.push(json!({"phase":"execution_progress","progress":progress}));
                if matches!(progress, ExecutionProgress::Completed) {
                    break;
                }
            }
            Err(e) => {
                events.push(json!({"phase":"execution_stopped","record":executor.record()}));
                return Err(e.to_string());
            }
        }
    }
    events.push(json!({"phase":"complete_roof","record":executor.record()}));
    Ok(())
}
