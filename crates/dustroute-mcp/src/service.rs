use std::collections::HashMap;
use std::sync::Arc;

use dustroute_app::DustRouteService;
use dustroute_translate::{ForwardOptions, ReverseRequest, world_from_snapshot_json};
use rmcp::{
    ServerHandler,
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
        wrapper::Parameters,
    },
    model::{GetPromptResult, Implementation, PromptMessage, Role, ServerCapabilities, ServerInfo},
    prompt, prompt_handler, prompt_router, schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::McpConfig;
use crate::{
    BotBridge, McpPolicy, OperationKind, OperationRegistry, OperationStatus, PlacementPlan,
    SelectionSession, discover_connected_region, plan_world_overlay,
};

#[derive(Clone)]
pub struct DustRouteMcp {
    bridge: BotBridge,
    selections: Arc<Mutex<HashMap<String, SelectionSession>>>,
    selection_dimensions: Arc<Mutex<HashMap<String, String>>>,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
    plans: Arc<Mutex<HashMap<uuid::Uuid, PlacementPlan>>>,
    policy: McpPolicy,
    app: DustRouteService,
    operations: OperationRegistry,
    assist_player: Option<String>,
    server_address: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PlayerParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    player: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ObserveParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    player: Option<String>,
    /// Ray-cast limit in blocks. Defaults to 64.
    max_distance: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct MarkCornerParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    player: Option<String>,
    /// Either `first` or `second`.
    corner: String,
    /// Ray-cast limit in blocks. Defaults to 64.
    max_distance: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DiscoverCircuitParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    player: Option<String>,
    /// Horizontal half-size of the initial scan, from 4 through 31. Defaults to 24.
    horizontal_radius: Option<i32>,
    /// Vertical half-size of the initial scan, from 2 through 16. Defaults to 12.
    vertical_radius: Option<i32>,
    /// Extra blocks around the discovered circuit. Defaults to 1.
    padding: Option<i32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PreviewPlacementParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    player: Option<String>,
    /// Built-in circuit: half-adder, half-subtractor, mux2, decoder1to2, or full-adder.
    circuit: String,
    /// Maximum number of blocks allowed in one placement plan. Defaults to 32768.
    max_blocks: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct OperationParams {
    /// Operation UUID returned by a start or preview tool.
    operation_id: String,
}

fn json_text(value: Value) -> String {
    serde_json::to_string_pretty(&value)
        .unwrap_or_else(|error| json!({ "ok": false, "error": error.to_string() }).to_string())
}

fn bounds_json(bounds: dustroute_translate::RegionBounds) -> Value {
    json!({ "min": bounds.min, "max": bounds.max })
}

fn reverse_result_json(
    bounds: dustroute_translate::RegionBounds,
    translated: &dustroute_translate::ReverseResult,
) -> Value {
    json!({
        "ok": true,
        "bounds": bounds_json(bounds),
        "redstone_blocks": translated.analysis.redstone_blocks.len(),
        "inputs": translated.analysis.inputs.iter().map(|terminal| json!({
            "position": terminal.anchor,
            "component": terminal.component,
            "confidence": format!("{:?}", terminal.confidence).to_lowercase(),
        })).collect::<Vec<_>>(),
        "outputs": translated.analysis.outputs.iter().map(|terminal| json!({
            "position": terminal.anchor,
            "component": terminal.component,
            "confidence": format!("{:?}", terminal.confidence).to_lowercase(),
        })).collect::<Vec<_>>(),
        "expressions": translated.expressions.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "truth_table": translated.truth_table.as_ref().map(|table| table.rows.iter().map(|row| json!({
            "inputs": row.inputs,
            "outputs": row.outputs,
        })).collect::<Vec<_>>()),
        "truth_table_error": translated.truth_table_error.as_ref().map(ToString::to_string),
        "diagnostics": {
            "signal_islands": translated.analysis.diagnostics.signal_islands.len(),
            "isolated_redstone": translated.analysis.diagnostics.isolated_redstone.len(),
            "unreachable_components": translated.analysis.diagnostics.unreachable_from_inputs.len(),
            "components_without_output_path": translated.analysis.diagnostics.cannot_reach_outputs.len(),
            "invalid_supports": translated.analysis.diagnostics.invalid_supports.len(),
            "non_controllable_torches": translated.analysis.diagnostics.non_controllable_torches.len(),
        }
    })
}

impl DustRouteMcp {
    #[must_use]
    pub fn new(bridge_address: impl Into<String>) -> Self {
        Self::with_policy(bridge_address, McpPolicy::default())
    }

    #[must_use]
    pub fn with_policy(bridge_address: impl Into<String>, policy: McpPolicy) -> Self {
        Self {
            bridge: BotBridge::new(bridge_address),
            selections: Arc::new(Mutex::new(HashMap::new())),
            selection_dimensions: Arc::new(Mutex::new(HashMap::new())),
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
            plans: Arc::new(Mutex::new(HashMap::new())),
            policy,
            app: DustRouteService::default(),
            operations: OperationRegistry::default(),
            assist_player: None,
            server_address: None,
        }
    }

    #[must_use]
    pub fn with_policy_and_player(
        bridge_address: impl Into<String>,
        policy: McpPolicy,
        assist_player: impl Into<String>,
    ) -> Self {
        let mut service = Self::with_policy(bridge_address, policy);
        service.assist_player = Some(assist_player.into());
        service
    }

    #[must_use]
    pub fn with_config(config: McpConfig, policy: McpPolicy) -> Self {
        let mut service =
            Self::with_policy_and_player(config.bridge_address, policy, config.assist_player);
        service.server_address = Some(config.server_address);
        service
    }

    fn resolve_player(&self, requested: Option<&str>) -> Result<String, String> {
        match (&self.assist_player, requested) {
            (Some(configured), None) => Ok(configured.clone()),
            (Some(configured), Some(requested)) if configured == requested => {
                Ok(configured.clone())
            }
            (Some(configured), Some(_)) => Err(format!(
                "player override is not allowed; configured assist player is {configured:?}"
            )),
            (None, Some(requested)) => Ok(requested.to_owned()),
            (None, None) => {
                Err("player is required when DUSTROUTE_ASSIST_PLAYER is not configured".to_owned())
            }
        }
    }

    fn authorize_player(&self, player: &str) -> Option<String> {
        self.policy
            .authorize_player(player)
            .err()
            .map(|error| json_text(json!({ "ok": false, "error": error.to_string() })))
    }

    async fn selected_region(
        &self,
        player: &str,
    ) -> Result<(dustroute_translate::RegionBounds, String), String> {
        let bounds = self
            .selections
            .lock()
            .await
            .get(player)
            .ok_or_else(|| "no selection session for player".to_owned())?
            .bounds()
            .map_err(|error| error.to_string())?;
        let dimension = self
            .selection_dimensions
            .lock()
            .await
            .get(player)
            .cloned()
            .ok_or_else(|| "selection has no dimension".to_owned())?;
        Ok((bounds, dimension))
    }
}

#[tool_router]
impl DustRouteMcp {
    #[tool(
        description = "Get the visible Minecraft bot connection status",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn server_status(&self) -> String {
        match self.bridge.status().await {
            Ok(status) => json_text(json!({
                "ok": true,
                "bot": status,
                "configured_server": self.server_address,
                "assist_player": self.assist_player,
                "policy": self.policy
            })),
            Err(error) => json_text(json!({ "ok": false, "error": error.to_string() })),
        }
    }

    #[tool(
        description = "List players currently visible to the Minecraft bot so gaze tools can use an exact player name",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn list_visible_players(&self) -> String {
        match self.bridge.visible_players().await {
            Ok(players) => {
                let players = players
                    .into_iter()
                    .filter(|player| {
                        self.policy.authorize_player(&player.player).is_ok()
                            && self.policy.authorize_dimension(&player.dimension).is_ok()
                    })
                    .collect::<Vec<_>>();
                json_text(json!({ "ok": true, "players": players }))
            }
            Err(error) => json_text(json!({ "ok": false, "error": error.to_string() })),
        }
    }

    #[tool(
        description = "Observe a player's eye position, gaze direction, and targeted block",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn observe_player(&self, Parameters(params): Parameters<ObserveParams>) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        match self
            .bridge
            .observe_player(&player, params.max_distance.unwrap_or(64.0))
            .await
        {
            Ok(observation) => match self.policy.authorize_dimension(&observation.dimension) {
                Ok(()) => json_text(json!({ "ok": true, "observation": observation })),
                Err(error) => json_text(json!({ "ok": false, "error": error.to_string() })),
            },
            Err(error) => json_text(json!({ "ok": false, "error": error.to_string() })),
        }
    }

    #[tool(
        description = "Mark the first or second region corner at the block a player is looking at"
    )]
    async fn mark_region_corner(&self, Parameters(params): Parameters<MarkCornerParams>) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        let observation = match self
            .bridge
            .observe_player(&player, params.max_distance.unwrap_or(64.0))
            .await
        {
            Ok(observation) => observation,
            Err(error) => {
                return json_text(json!({ "ok": false, "error": error.to_string() }));
            }
        };
        let Some(target) = observation.targeted_block else {
            return json_text(
                json!({ "ok": false, "error": "the player is not looking at a block" }),
            );
        };
        if let Err(error) = self.policy.authorize_dimension(&observation.dimension) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let mut selections = self.selections.lock().await;
        let session = selections
            .entry(player.clone())
            .or_insert_with(|| SelectionSession::new(&player));
        let result = match params.corner.as_str() {
            "first" => {
                session.mark_first(target);
                self.selection_dimensions
                    .lock()
                    .await
                    .insert(player.clone(), observation.dimension.clone());
                json!({ "ok": true, "corner": "first", "position": target })
            }
            "second" => match self
                .selection_dimensions
                .lock()
                .await
                .get(&player)
                .filter(|dimension| *dimension == &observation.dimension)
            {
                None => {
                    json!({ "ok": false, "error": "player changed dimension between region corners" })
                }
                Some(_) => match session.mark_second(target) {
                    Ok(bounds) => {
                        json!({ "ok": true, "corner": "second", "position": target, "bounds": bounds_json(bounds) })
                    }
                    Err(error) => json!({ "ok": false, "error": error.to_string() }),
                },
            },
            _ => json!({ "ok": false, "error": "corner must be first or second" }),
        };
        json_text(result)
    }

    #[tool(
        description = "Infer the bounds of 'this circuit' by following redstone connected to the block a player is looking at"
    )]
    async fn discover_looked_at_circuit(
        &self,
        Parameters(params): Parameters<DiscoverCircuitParams>,
    ) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        let horizontal = params.horizontal_radius.unwrap_or(24);
        let vertical = params.vertical_radius.unwrap_or(12);
        let padding = params.padding.unwrap_or(1);
        if !(4..=31).contains(&horizontal)
            || !(2..=16).contains(&vertical)
            || !(0..=8).contains(&padding)
        {
            return json_text(json!({
                "ok": false,
                "error": "horizontal_radius must be 4..31, vertical_radius 2..16, and padding 0..8"
            }));
        }
        let observation = match self.bridge.observe_player(&player, 64.0).await {
            Ok(observation) => observation,
            Err(error) => {
                return json_text(json!({ "ok": false, "error": error.to_string() }));
            }
        };
        let Some(target) = observation.targeted_block else {
            return json_text(
                json!({ "ok": false, "error": "the player is not looking at a block" }),
            );
        };
        if let Err(error) = self.policy.authorize_dimension(&observation.dimension) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let scan_bounds = dustroute_translate::RegionBounds::new(
            dustroute_model::Pos::new(
                target.x - horizontal,
                target.y - vertical,
                target.z - horizontal,
            ),
            dustroute_model::Pos::new(
                target.x + horizontal,
                target.y + vertical,
                target.z + horizontal,
            ),
        );
        if let Err(error) = self.policy.validate_region(scan_bounds) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let snapshot = match self
            .bridge
            .scan_region(scan_bounds.min, scan_bounds.max, &observation.dimension)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return json_text(json!({ "ok": false, "error": error.to_string() }));
            }
        };
        let snapshot_json = match serde_json::to_string(&snapshot) {
            Ok(json) => json,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let (_, world) = match world_from_snapshot_json(&snapshot_json) {
            Ok(result) => result,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let analysis = dustroute_translate::analyze_world_region(&world, scan_bounds);
        let discovery = match discover_connected_region(&analysis, target, 2, padding, 8192) {
            Ok(discovery) => discovery,
            Err(error) => {
                return json_text(json!({ "ok": false, "error": error.to_string() }));
            }
        };
        let bounds: dustroute_translate::RegionBounds = discovery.bounds.into();
        self.selections
            .lock()
            .await
            .entry(player.clone())
            .or_insert_with(|| SelectionSession::new(&player))
            .set_bounds(bounds);
        self.selection_dimensions
            .lock()
            .await
            .insert(player, observation.dimension);
        json_text(json!({
            "ok": true,
            "candidate": discovery,
            "warning": discovery.touches_scan_boundary.then_some(
                "the connected circuit touches the initial scan boundary; increase the scan radius before confirming"
            ),
            "next_step": "call preview_region and ask the player to confirm the highlighted candidate"
        }))
    }

    #[tool(
        description = "Compile a built-in circuit at a player's gaze target and return a read-only block diff, collisions, materials, operation ID, and undo plan"
    )]
    async fn preview_compiled_circuit(
        &self,
        Parameters(params): Parameters<PreviewPlacementParams>,
    ) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        let observation = match self.bridge.observe_player(&player, 64.0).await {
            Ok(observation) => observation,
            Err(error) => {
                return json_text(json!({ "ok": false, "error": error.to_string() }));
            }
        };
        let Some(origin) = observation.targeted_block else {
            return json_text(
                json!({ "ok": false, "error": "the player is not looking at a block" }),
            );
        };
        if let Err(error) = self.policy.authorize_dimension(&observation.dimension) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let translated = match self
            .app
            .compile_builtin(&params.circuit, ForwardOptions::default())
        {
            Ok(Some(result)) => result,
            Ok(None) => {
                return json_text(json!({
                    "ok": false,
                    "error": "unknown circuit; expected half-adder, half-subtractor, mux2, decoder1to2, or full-adder"
                }));
            }
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let Some((local_min, local_max)) = translated.compiled.world.bounds() else {
            return json_text(json!({ "ok": false, "error": "compiled circuit is empty" }));
        };
        let min = dustroute_model::Pos::new(
            local_min.x + origin.x,
            local_min.y + origin.y,
            local_min.z + origin.z,
        );
        let max = dustroute_model::Pos::new(
            local_max.x + origin.x,
            local_max.y + origin.y,
            local_max.z + origin.z,
        );
        let placement_bounds = dustroute_translate::RegionBounds::new(min, max);
        if let Err(error) = self.policy.validate_region(placement_bounds) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let proposed_blocks = translated.compiled.world.iter().count();
        if let Err(error) = self.policy.validate_placement_size(proposed_blocks) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let snapshot = match self
            .bridge
            .scan_region(min, max, &observation.dimension)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return json_text(json!({ "ok": false, "error": error.to_string() }));
            }
        };
        let snapshot_json = match serde_json::to_string(&snapshot) {
            Ok(json) => json,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let (_, existing) = match world_from_snapshot_json(&snapshot_json) {
            Ok(result) => result,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let plan = match plan_world_overlay(
            &existing,
            &translated.compiled.world,
            origin,
            params
                .max_blocks
                .unwrap_or(self.policy.max_placement_blocks)
                .min(self.policy.max_placement_blocks),
        ) {
            Ok(plan) => plan,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let operation_id = plan.operation_id;
        let collision_samples = plan
            .changes
            .iter()
            .filter(|change| change.collision)
            .take(32)
            .map(|change| change.pos)
            .collect::<Vec<_>>();
        let response = json!({
            "ok": true,
            "read_only": true,
            "operation_id": operation_id,
            "origin": origin,
            "bounds": { "min": min, "max": max },
            "changed_blocks": plan.changes.len(),
            "collision_count": plan.collision_count,
            "collision_samples": collision_samples,
            "materials": &plan.materials,
            "undo_change_count": plan.undo.changes.len(),
            "next_step": "review this plan; no blocks have been changed"
        });
        self.plans.lock().await.insert(operation_id, plan);
        self.operations
            .record_completed(
                operation_id,
                OperationKind::PlacementPreview,
                response.clone(),
            )
            .await;
        json_text(response)
    }

    #[tool(
        description = "Retrieve the complete read-only placement and undo plan by operation ID",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_placement_plan(&self, Parameters(params): Parameters<OperationParams>) -> String {
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        match self.plans.lock().await.get(&operation_id) {
            Some(plan) => json_text(json!({ "ok": true, "read_only": true, "plan": plan })),
            None => json_text(json!({ "ok": false, "error": "unknown operation ID" })),
        }
    }

    #[tool(
        description = "Show the player's selected region in the Minecraft world before analysis or mutation"
    )]
    async fn preview_region(&self, Parameters(params): Parameters<PlayerParams>) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        let (bounds, dimension) = match self.selected_region(&player).await {
            Ok(region) => region,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Err(error) = self.policy.validate_region(bounds) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        match self
            .bridge
            .preview_region(&player, bounds.min, bounds.max, &dimension)
            .await
        {
            Ok(preview) => {
                json_text(json!({ "ok": true, "bounds": bounds_json(bounds), "preview": preview }))
            }
            Err(error) => json_text(json!({ "ok": false, "error": error.to_string() })),
        }
    }

    #[tool(
        description = "Scan and reverse-translate the region previously selected with the player's gaze"
    )]
    async fn analyze_selected_region(
        &self,
        Parameters(params): Parameters<PlayerParams>,
    ) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        let (bounds, dimension) = match self.selected_region(&player).await {
            Ok(region) => region,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Err(error) = self.policy.validate_region(bounds) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let snapshot = match self
            .bridge
            .scan_region(bounds.min, bounds.max, &dimension)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return json_text(json!({ "ok": false, "error": error.to_string() }));
            }
        };
        let snapshot_json = match serde_json::to_string(&snapshot) {
            Ok(json) => json,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let (_, world) = match world_from_snapshot_json(&snapshot_json) {
            Ok(result) => result,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let translated = self.app.analyze_world(&world, ReverseRequest::new(bounds));
        json_text(reverse_result_json(bounds, &translated))
    }

    #[tool(
        description = "Start cancellable reverse analysis of the selected region and return an operation ID for progress polling"
    )]
    async fn start_selected_region_analysis(
        &self,
        Parameters(params): Parameters<PlayerParams>,
    ) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        let (bounds, dimension) = match self.selected_region(&player).await {
            Ok(region) => region,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Err(error) = self.policy.validate_region(bounds) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let operation_id = self
            .operations
            .create(OperationKind::AnalyzeRegion, "queued for snapshot scan")
            .await;
        let operations = self.operations.clone();
        let bridge = self.bridge.clone();
        let app = self.app;
        tokio::spawn(async move {
            operations
                .update(
                    operation_id,
                    OperationStatus::Running,
                    10,
                    "scanning region",
                )
                .await;
            let snapshot = match bridge.scan_region(bounds.min, bounds.max, &dimension).await {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    operations.fail(operation_id, error.to_string()).await;
                    return;
                }
            };
            if operations.is_cancelled(operation_id).await {
                return;
            }
            operations
                .update(
                    operation_id,
                    OperationStatus::Running,
                    35,
                    "normalizing Minecraft snapshot",
                )
                .await;
            let snapshot_json = match serde_json::to_string(&snapshot) {
                Ok(json) => json,
                Err(error) => {
                    operations.fail(operation_id, error.to_string()).await;
                    return;
                }
            };
            let (_, world) = match world_from_snapshot_json(&snapshot_json) {
                Ok(result) => result,
                Err(error) => {
                    operations.fail(operation_id, error.to_string()).await;
                    return;
                }
            };
            operations
                .update(
                    operation_id,
                    OperationStatus::Running,
                    50,
                    "simulating truth table",
                )
                .await;
            let translated = match tokio::task::spawn_blocking(move || {
                app.analyze_world(&world, ReverseRequest::new(bounds))
            })
            .await
            {
                Ok(result) => result,
                Err(error) => {
                    operations.fail(operation_id, error.to_string()).await;
                    return;
                }
            };
            if operations.is_cancelled(operation_id).await {
                return;
            }
            operations
                .complete(operation_id, reverse_result_json(bounds, &translated))
                .await;
        });
        json_text(json!({
            "ok": true,
            "operation_id": operation_id,
            "status": "queued",
            "next_step": "poll get_operation; call cancel_operation if the analysis is no longer needed"
        }))
    }

    #[tool(
        description = "Get progress and result for a long-running DustRoute operation",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_operation(&self, Parameters(params): Parameters<OperationParams>) -> String {
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        match self.operations.get(operation_id).await {
            Some(operation) => json_text(json!({ "ok": true, "operation": operation })),
            None => json_text(json!({ "ok": false, "error": "unknown operation ID" })),
        }
    }

    #[tool(
        description = "List analysis and placement-preview operation history",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn list_operations(&self) -> String {
        json_text(json!({ "ok": true, "operations": self.operations.list().await }))
    }

    #[tool(description = "Cancel a queued or running DustRoute operation")]
    async fn cancel_operation(&self, Parameters(params): Parameters<OperationParams>) -> String {
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let cancelled = self.operations.cancel(operation_id).await;
        json_text(json!({ "ok": cancelled, "operation_id": operation_id }))
    }

    #[tool(description = "Clear a player's pending gaze-based region selection")]
    async fn clear_region_selection(&self, Parameters(params): Parameters<PlayerParams>) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        if let Some(session) = self.selections.lock().await.get_mut(&player) {
            session.clear();
        }
        self.selection_dimensions.lock().await.remove(&player);
        json_text(json!({ "ok": true, "player": player }))
    }
}

#[prompt_router]
impl DustRouteMcp {
    #[prompt(
        name = "collaborate-on-redstone-circuit",
        description = "Ground natural-language references in a player's gaze and safely inspect a redstone circuit"
    )]
    async fn collaboration_prompt(&self) -> GetPromptResult {
        GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            "Work with the player on a Minecraft redstone circuit. Interpret 'this', 'here', and similar deictic phrases through observe_player. For 'this circuit', call discover_looked_at_circuit, then preview_region and ask the player to confirm the highlighted bounds before analysis. For 'from here to there', call mark_region_corner for first and second while the player looks at each point, then preview. If discovery reports that it touches the scan boundary, increase the scan radius before treating the bounds as complete. Read-only observation and analysis may proceed after range confirmation. Never infer coordinates from prose when gaze tools can ground them, and never perform a world mutation without an explicit preview and confirmation.".to_owned(),
        )])
        .with_description("Safe gaze-grounded DustRoute collaboration workflow")
    }
}

#[tool_handler(router = self.tool_router)]
#[prompt_handler(router = self.prompt_router)]
impl ServerHandler for DustRouteMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
        )
        .with_server_info(Implementation::new(
            "dustroute-mcp",
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(
            "Use the collaborate-on-redstone-circuit prompt. Ground relative language in player gaze, preview ranges before analysis, and keep world operations read-only unless policy and explicit confirmation permit otherwise."
        )
    }
}

#[cfg(test)]
mod tests {
    use rmcp::{
        ServiceExt,
        model::{CallToolRequestParams, ClientInfo, ContentBlock},
    };
    use serde_json::{Value, json};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    use super::*;

    #[tokio::test]
    async fn collaboration_prompt_requires_gaze_grounding_and_preview() {
        let prompt = DustRouteMcp::new("127.0.0.1:1")
            .collaboration_prompt()
            .await;
        let ContentBlock::Text(text) = &prompt.messages[0].content else {
            panic!("expected text prompt");
        };
        assert!(text.text.contains("discover_looked_at_circuit"));
        assert!(text.text.contains("preview_region"));
        assert!(text.text.contains("confirmation"));
    }

    #[tokio::test]
    async fn exposes_gaze_tools_prompt_and_fake_bot_status_over_mcp() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = String::new();
            BufReader::new(&mut stream)
                .read_line(&mut request)
                .await
                .unwrap();
            let request: Value = serde_json::from_str(&request).unwrap();
            let response = json!({
                "id": request["id"],
                "result": {
                    "connected": true,
                    "username": "DustRouteBot",
                    "host": "test",
                    "port": 25565,
                    "version": "1.21.11",
                    "dimension": "minecraft:overworld"
                }
            });
            stream
                .write_all(format!("{response}\n").as_bytes())
                .await
                .unwrap();
        });

        let (server_transport, client_transport) = tokio::io::duplex(16 * 1024);
        let server = tokio::spawn(async move {
            DustRouteMcp::new(address)
                .serve(server_transport)
                .await
                .unwrap()
                .waiting()
                .await
                .unwrap();
        });
        let client = ClientInfo::default().serve(client_transport).await.unwrap();
        let tools = client.list_tools(None).await.unwrap();
        assert!(
            tools
                .tools
                .iter()
                .any(|tool| tool.name == "discover_looked_at_circuit")
        );
        let prompts = client.list_prompts(None).await.unwrap();
        assert!(
            prompts
                .prompts
                .iter()
                .any(|prompt| prompt.name == "collaborate-on-redstone-circuit")
        );
        let result = client
            .call_tool(CallToolRequestParams::new("server_status"))
            .await
            .unwrap();
        let ContentBlock::Text(text) = &result.content[0] else {
            panic!("expected text tool result");
        };
        assert!(text.text.contains("DustRouteBot"));
        client.cancel().await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn runs_two_gaze_points_preview_and_reverse_analysis_over_mcp() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        tokio::spawn(async move {
            let mut observation = 0;
            for _ in 0..4 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = String::new();
                BufReader::new(&mut stream)
                    .read_line(&mut request)
                    .await
                    .unwrap();
                let request: Value = serde_json::from_str(&request).unwrap();
                let result = match request["method"].as_str().unwrap() {
                    "observe_player" => {
                        let target = if observation == 0 {
                            json!({ "x": 0, "y": 64, "z": 0 })
                        } else {
                            json!({ "x": 3, "y": 66, "z": 1 })
                        };
                        observation += 1;
                        json!({
                            "player": "builder",
                            "eye_position": { "x": 0.5, "y": 65.62, "z": 4.5 },
                            "yaw": 0.0,
                            "pitch": 0.0,
                            "targeted_block": target,
                            "targeted_face": "up",
                            "distance": 4.0,
                            "dimension": "minecraft:overworld"
                        })
                    }
                    "preview_region" => json!({ "particle_corners": 8 }),
                    "scan_region" => json!({
                        "min": { "x": 0, "y": 64, "z": 0 },
                        "max": { "x": 3, "y": 66, "z": 1 },
                        "blocks": []
                    }),
                    method => panic!("unexpected fake bridge method {method}"),
                };
                let response = json!({ "id": request["id"], "result": result });
                stream
                    .write_all(format!("{response}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });

        let (server_transport, client_transport) = tokio::io::duplex(64 * 1024);
        let server = tokio::spawn(async move {
            DustRouteMcp::with_policy_and_player(address, McpPolicy::default(), "builder")
                .serve(server_transport)
                .await
                .unwrap()
                .waiting()
                .await
                .unwrap();
        });
        let client = ClientInfo::default().serve(client_transport).await.unwrap();
        for corner in ["first", "second"] {
            let arguments = serde_json::from_value(json!({
                "corner": corner
            }))
            .unwrap();
            let result = client
                .call_tool(
                    CallToolRequestParams::new("mark_region_corner").with_arguments(arguments),
                )
                .await
                .unwrap();
            let ContentBlock::Text(text) = &result.content[0] else {
                panic!("expected text tool result");
            };
            assert!(text.text.contains("\"ok\": true"));
        }
        for tool in ["preview_region", "analyze_selected_region"] {
            let arguments = serde_json::Map::new();
            let result = client
                .call_tool(CallToolRequestParams::new(tool).with_arguments(arguments))
                .await
                .unwrap();
            let ContentBlock::Text(text) = &result.content[0] else {
                panic!("expected text tool result");
            };
            assert!(text.text.contains("\"ok\": true"));
        }
        client.cancel().await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn rejects_an_override_of_the_configured_assist_player() {
        let service =
            DustRouteMcp::with_policy_and_player("127.0.0.1:1", McpPolicy::default(), "builder");
        let result = service
            .observe_player(Parameters(ObserveParams {
                player: Some("someone_else".to_owned()),
                max_distance: None,
            }))
            .await;
        assert!(result.contains("player override is not allowed"));
    }
}
