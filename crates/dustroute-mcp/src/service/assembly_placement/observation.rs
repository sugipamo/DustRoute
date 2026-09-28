//! Complete live snapshots and their evidence limits; never reconstructs runtime history.
use super::*;

pub(super) fn stable_baseline(
    observation: &Value,
) -> Result<dustroute_translate::MinecraftSnapshot, String> {
    if !matches!(observation["status"].as_str(), Some("matches" | "changed")) {
        return Err(format!(
            "operation needs complete, unchanged samples: {observation}"
        ));
    }
    serde_json::from_value(observation["snapshot"].clone()).map_err(|e| e.to_string())
}

impl DustRouteMcp {
    pub(super) async fn observe_instance(&self, record: &PlacedAssembly) -> Value {
        let bounds =
            dustroute_translate::RegionBounds::new(record.expected.min, record.expected.max);
        let observed: Result<Value,String> = async {
            self.policy.authorize_dimension(&record.target.dimension).map_err(|e|e.to_string())?;
            self.policy.validate_region(bounds).map_err(|e|e.to_string())?;
            let status = self.bridge.status().await.map_err(|e|e.to_string())?;
            if let Err(error) = record.target.check(&status).and_then(|()|server_contract(&status,&record.target.dimension)) {
                return Ok(json!({"status":"target_mismatch","reason":error}));
            }
            let first_readback = self.bridge.scan_region_confirmed(bounds.min,bounds.max,&record.target.dimension).await.map_err(|e|e.to_string())?;
            let first = first_readback.snapshot;
            // Two complete, separated observations expose ongoing changes. This
            // is a stability check, not reconstruction of hidden scheduled ticks.
            self.bridge.wait_ticks(20,&record.target.dimension).await.map_err(|e|e.to_string())?;
            let status = self.bridge.status().await.map_err(|e|e.to_string())?;
            record.target.check(&status)?;
            let second_readback = self.bridge.scan_region_confirmed(bounds.min,bounds.max,&record.target.dimension).await.map_err(|e|e.to_string())?;
            let second = second_readback.snapshot;
            let server_interval = second_readback.readback.start_game_tick.checked_sub(first_readback.readback.end_game_tick)
                .ok_or("server clock moved backwards between observations")?;
            for sample in [&first, &second] {
                if sample.min != bounds.min || sample.max != bounds.max { return Err("exact complete observation region required".into()); }
                crate::revision::blocks(sample)?;
            }
            let moving = [&first,&second].iter().any(|s|s.blocks.iter().any(|b|b.name == "minecraft:moving_piston"));
            if moving || ValidatedAssemblyPlacement::matches(&second,&first,&status.version).is_err() {
                return Ok(json!({"status":"history_unavailable","reason":"moving or changing observation; exact pending callbacks and motion history cannot be recovered from snapshots","samples":[first,second],"readbacks":[first_readback.readback,second_readback.readback],"sample_interval_ticks":20,"sample_interval_clock":"client","observed_server_tick_interval":server_interval}));
            }
            let expected = if record.state == InstanceState::Removed {
                dustroute_translate::MinecraftSnapshot { min:bounds.min,max:bounds.max,blocks:vec![] }
            } else { record.expected.clone() };
            let matches = ValidatedAssemblyPlacement::matches(&second,&expected,&status.version);
            Ok(json!({"status":if matches.is_ok(){"matches"}else{"changed"},"reason":matches.err(),"snapshot":second,"readbacks":[first_readback.readback,second_readback.readback],"sample_interval_ticks":20,"sample_interval_clock":"client","observed_server_tick_interval":server_interval,"matching_samples":2}))
        }.await;
        let mut result = observed
            .unwrap_or_else(|reason| json!({"status":"observation_incomplete","reason":reason}));
        result["observed_at_unix_ms"] = json!(now_ms().ok());
        result["runtime_history_reconstructed"] = json!(false);
        result
    }
}
