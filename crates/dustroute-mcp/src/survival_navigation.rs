//! Bounded, read-only route selection using the version adapter's physics.
//! This is a player plan, not Blueprint adoption or a durable construction job.
use rmcp::schemars;
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap};
use voxrig::checked_survival::{
    HypotheticalMovementPreview, MAX_SURVIVAL_CONTROL_TICKS, Operations, PredictedMotionFrame,
    SurvivalControl, SurvivalInput, SurvivalMotionRecord, SurvivalMovementPreview,
    SurvivalScenario, TerminalClearance,
};

/// Reviewed spatial scope, inclusive bounds on the entire standing/jumping body.
#[derive(
    Clone, Copy, Debug, PartialEq, Serialize, serde::Deserialize, rmcp::schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct TravelBounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}
/// Feet-position goal volume within a declared travel scope.
#[derive(Clone, Debug, Serialize)]
pub struct RouteRequest {
    pub travel: TravelBounds,
    pub goal: TravelBounds,
    /// Includes rejected predictions and return-path checks; 2..4096.
    pub max_previews: usize,
}
/// Bounded search evidence. Exhaustion never proves that no physical route exists.
#[derive(Clone, Debug, Default, Serialize)]
pub struct RouteSearch {
    pub previews: usize,
    pub expanded: usize,
    pub frontier_remaining: usize,
    pub rejected: usize,
    /// At most sixteen distinct causes; counts are diagnostic, not proofs.
    pub refusal_examples: BTreeMap<String, usize>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RouteFailure {
    InvalidRequest { reason: String },
    ObservationChanged { search: RouteSearch },
    ObservationUnavailable { reason: String, search: RouteSearch },
    NoRouteWithinLimits { search: RouteSearch },
}
/// Not Deserialize: serialized diagnostics cannot restore an executable route.
#[derive(Clone, Debug, Serialize)]
pub struct SurvivalRoute<P = SurvivalMovementPreview> {
    request: RouteRequest,
    outbound: P,
    /// Full outbound + return prediction, against the original world snapshot.
    /// The return must be re-planned after any construction/world edit.
    round_trip: P,
    return_starts_at_tick: usize,
    search: RouteSearch,
}
impl<P> SurvivalRoute<P> {
    pub fn outbound(&self) -> &P {
        &self.outbound
    }
    pub fn round_trip(&self) -> &P {
        &self.round_trip
    }
    pub fn search(&self) -> &RouteSearch {
        &self.search
    }
}
/// A future-world route. It deliberately has no live `start` method.
pub type HypotheticalRoute = SurvivalRoute<HypotheticalMovementPreview>;
impl HypotheticalRoute {
    pub fn return_controls(&self) -> &[SurvivalControl] {
        &self.round_trip.controls[self.return_starts_at_tick..]
    }
    /// Advance only the exact immutable scenario used for this route's search.
    /// Any intervening hypothetical edit requires a new route search.
    pub fn after_outbound(
        &self,
        scenario: &SurvivalScenario,
    ) -> Result<SurvivalScenario, RouteFailure> {
        if !scenario.matches_preview(&self.outbound) {
            return Err(RouteFailure::ObservationChanged {
                search: self.search.clone(),
            });
        }
        scenario.after_path(&self.outbound.controls).map_err(|e| {
            RouteFailure::ObservationUnavailable {
                reason: e.to_string(),
                search: self.search.clone(),
            }
        })
    }
}
impl SurvivalRoute {
    pub fn return_controls(&self) -> &[SurvivalControl] {
        &self.round_trip.controls[self.return_starts_at_tick..]
    }
    /// Recompute the return after placement/world changes, requiring the exact
    /// outbound endpoint and same connection/generation. This sends no input.
    pub async fn preview_return(
        &self,
        mover: &Operations,
    ) -> Result<SurvivalMovementPreview, RouteFailure> {
        let stats = RouteSearch {
            previews: 1,
            ..Default::default()
        };
        let preview = mover
            .preview_survival_path(self.return_controls())
            .await
            .map_err(|e| RouteFailure::ObservationUnavailable {
                reason: e.to_string(),
                search: stats.clone(),
            })?;
        if preview.initial.connection_id != self.outbound.initial.connection_id
            || preview.generation != self.outbound.generation
            || preview.initial.dimension != self.outbound.initial.dimension
            || preview.initial.position != self.outbound.frames.last().unwrap().position
        {
            return Err(RouteFailure::ObservationChanged { search: stats });
        }
        let end = preview.frames.last().unwrap().position;
        let origin = self.outbound.initial.position;
        if !admissible(&preview, self.request.travel)
            || (end[1] - origin[1]).abs() >= 0.125
            || (end[0] - origin[0]).hypot(end[2] - origin[2]) > 0.35
        {
            return Err(RouteFailure::NoRouteWithinLimits { search: stats });
        }
        Ok(preview)
    }
    /// Native revalidation occurs under the intent lock before any control send.
    /// Caller still owns edit/site authorization and durable job intent storage.
    pub async fn start(
        &self,
        mover: &Operations,
        observer: &Operations,
    ) -> voxrig::Result<SurvivalMotionRecord> {
        mover
            .start_previewed_survival_motion(&self.outbound, observer)
            .await
    }
}
#[derive(Clone, Copy)]
struct Primitive {
    yaw: f32,
    active: usize,
    jump: bool,
}
impl Primitive {
    fn append(self, controls: &mut Vec<SurvivalControl>) {
        let released = if self.jump { 20 } else { 12 };
        controls.extend((0..self.active + released).map(|tick| SurvivalControl {
            yaw: self.yaw,
            input: SurvivalInput {
                forward: i8::from(tick < self.active),
                strafe: 0,
                jump: self.jump && tick == 0,
            },
        }));
    }
    fn reversed(self) -> Self {
        Self {
            yaw: if self.yaw >= 0.0 {
                self.yaw - 180.0
            } else {
                self.yaw + 180.0
            },
            ..self
        }
    }
}
struct Node {
    controls: Vec<SurvivalControl>,
    primitives: Vec<Primitive>,
    position: [f64; 3],
}
fn contains(b: TravelBounds, p: [f64; 3]) -> bool {
    (0..3).all(|i| p[i] >= b.min[i] && p[i] <= b.max[i])
}
// The search consumes native predictions without converting hypothetical
// frames into received/live contexts or maintaining another physics model.
trait Prediction {
    fn origin(&self) -> [f64; 3];
    fn bounds(&self) -> [f64; 6];
    fn frames(&self) -> &[PredictedMotionFrame];
    fn terminal(&self) -> &TerminalClearance;
    fn same_origin(&self, other: &Self) -> bool;
}
impl Prediction for SurvivalMovementPreview {
    fn origin(&self) -> [f64; 3] {
        self.initial.position
    }
    fn bounds(&self) -> [f64; 6] {
        self.initial.bounds
    }
    fn frames(&self) -> &[PredictedMotionFrame] {
        &self.frames
    }
    fn terminal(&self) -> &TerminalClearance {
        &self.terminal_clearance
    }
    fn same_origin(&self, other: &Self) -> bool {
        same_initial(self, other)
    }
}
impl Prediction for HypotheticalMovementPreview {
    fn origin(&self) -> [f64; 3] {
        self.initial_position
    }
    fn bounds(&self) -> [f64; 6] {
        self.initial_bounds
    }
    fn frames(&self) -> &[PredictedMotionFrame] {
        &self.frames
    }
    fn terminal(&self) -> &TerminalClearance {
        &self.terminal_clearance
    }
    fn same_origin(&self, other: &Self) -> bool {
        self.shares_origin(other)
    }
}
fn within_scope(b: TravelBounds, p: [f64; 3], initial: &impl Prediction) -> bool {
    let min = std::array::from_fn(|i| p[i] + initial.bounds()[i] - initial.origin()[i]);
    let max = std::array::from_fn(|i| p[i] + initial.bounds()[i + 3] - initial.origin()[i]);
    contains(b, min) && contains(b, max)
}

fn goal_distance(goal: TravelBounds, p: [f64; 3]) -> f64 {
    (0..3)
        .map(|i| (p[i] - p[i].clamp(goal.min[i], goal.max[i])).powi(2))
        .sum::<f64>()
        .sqrt()
}
fn cell(p: [f64; 3]) -> [i32; 3] {
    p.map(|v| (v * 16.0).round() as i32)
}
fn same_initial(a: &SurvivalMovementPreview, b: &SurvivalMovementPreview) -> bool {
    a.generation == b.generation
        && a.initial.connection_id == b.initial.connection_id
        && a.initial.world_revision == b.initial.world_revision
        && a.initial.dimension == b.initial.dimension
        && a.initial.position == b.initial.position
        && a.initial.player == b.initial.player
}
pub(crate) fn validate(request: &RouteRequest) -> Result<(), RouteFailure> {
    let fail = |s: &str| RouteFailure::InvalidRequest { reason: s.into() };
    for b in [request.travel, request.goal] {
        if (0..3).any(|i| {
            !b.min[i].is_finite()
                || !b.max[i].is_finite()
                || b.min[i] > b.max[i]
                || b.min[i].abs() > 30_000_000.0
                || b.max[i].abs() > 30_000_000.0
        }) {
            return Err(fail("invalid route bounds"));
        }
    }
    if (0..3).any(|i| request.travel.max[i] - request.travel.min[i] > 32.0)
        || !contains(request.travel, request.goal.min)
        || !contains(request.travel, request.goal.max)
        || !(2..=4096).contains(&request.max_previews)
    {
        return Err(fail(
            "route requires bounded 32-block axes, contained goal and 2..4096 previews",
        ));
    }
    Ok(())
}
fn reject(search: &mut RouteSearch, reason: String) {
    search.rejected += 1;
    if search.refusal_examples.len() < 16 || search.refusal_examples.contains_key(&reason) {
        *search.refusal_examples.entry(reason).or_default() += 1;
    }
}
fn admissible(p: &impl Prediction, travel: TravelBounds) -> bool {
    matches!(p.terminal(), TerminalClearance::Admitted { .. })
        && within_scope(travel, p.origin(), p)
        && p.frames()
            .iter()
            .all(|f| within_scope(travel, f.position, p))
}
pub(crate) fn hypothetical_admissible(
    prediction: &HypotheticalMovementPreview,
    travel: TravelBounds,
) -> bool {
    admissible(prediction, travel)
}
/// Read-only route search on an immutable future-world branch. This uses the
/// same search and native movement kernel as live routes but grants no action
/// authority. Re-plan after edits; original round-trip evidence then expires.
pub async fn plan_hypothetical_route(
    scenario: &SurvivalScenario,
    request: RouteRequest,
) -> Result<HypotheticalRoute, RouteFailure> {
    search(request, |controls| {
        std::future::ready(scenario.preview_path(&controls))
    })
    .await
}
/// Search finite turn/walk/jump primitives with native prediction and an explicit
/// predicted return to the starting area. All candidates begin at the same live
/// context; a changed baseline aborts. No commands, inputs or world edits are sent.
/// Each route reserves half the native tick budget for a verified return. Search
/// bins endpoints at 1/16 block and is neither complete nor optimal.
pub async fn plan_survival_route(
    operations: &Operations,
    request: RouteRequest,
) -> Result<SurvivalRoute, RouteFailure> {
    search(request, |controls| async move {
        operations.preview_survival_path(&controls).await
    })
    .await
}
async fn search<F, Fut, P: Prediction>(
    request: RouteRequest,
    mut predict: F,
) -> Result<SurvivalRoute<P>, RouteFailure>
where
    F: FnMut(Vec<SurvivalControl>) -> Fut,
    Fut: std::future::Future<Output = voxrig::Result<P>>,
{
    validate(&request)?;
    let mut stats = RouteSearch {
        previews: 1,
        ..Default::default()
    };
    let idle = vec![
        SurvivalControl {
            yaw: 0.0,
            input: SurvivalInput::default()
        };
        3
    ];
    let baseline =
        predict(idle.clone())
            .await
            .map_err(|e| RouteFailure::ObservationUnavailable {
                reason: e.to_string(),
                search: stats.clone(),
            })?;
    let origin = baseline.origin();
    if !within_scope(request.travel, origin, &baseline) {
        return Err(RouteFailure::InvalidRequest {
            reason: "initial body lies outside travel scope".into(),
        });
    }
    if contains(request.goal, origin) && admissible(&baseline, request.travel) {
        let controls = [idle.clone(), idle].concat();
        stats.previews += 1;
        let round_trip =
            predict(controls)
                .await
                .map_err(|e| RouteFailure::ObservationUnavailable {
                    reason: e.to_string(),
                    search: stats.clone(),
                })?;
        if !baseline.same_origin(&round_trip) {
            return Err(RouteFailure::ObservationChanged { search: stats });
        }
        if admissible(&round_trip, request.travel) {
            return Ok(SurvivalRoute {
                request,
                outbound: baseline,
                round_trip,
                return_starts_at_tick: 3,
                search: stats,
            });
        }
    }
    let mut nodes = vec![Node {
        controls: Vec::new(),
        primitives: Vec::new(),
        position: origin,
    }];
    let mut frontier = BinaryHeap::from([Reverse((0u64, 0usize))]);
    let mut visited = HashMap::from([(cell(origin), 0usize)]);
    while let Some(Reverse((_, index))) = frontier.pop() {
        if stats.previews >= request.max_previews {
            break;
        }
        let node = &nodes[index];
        if visited
            .get(&cell(node.position))
            .is_some_and(|cost| *cost < node.controls.len())
        {
            continue;
        }
        let prefix = node.controls.clone();
        let primitives = node.primitives.clone();
        stats.expanded += 1;
        for yaw in [-135.0, -90.0, -45.0, 0.0, 45.0, 90.0, 135.0, 180.0] {
            for (active, jump) in [(4, false), (8, false), (4, true), (8, true)] {
                if stats.previews >= request.max_previews {
                    break;
                }
                let primitive = Primitive { yaw, active, jump };
                let mut controls = prefix.clone();
                primitive.append(&mut controls);
                if controls.len() > MAX_SURVIVAL_CONTROL_TICKS / 2 {
                    continue;
                }
                stats.previews += 1;
                // Let packet readers apply queued world/player updates between candidates.
                tokio::task::yield_now().await;
                let outbound = match predict(controls.clone()).await {
                    Ok(p) => p,
                    Err(e) => {
                        reject(&mut stats, e.to_string());
                        continue;
                    }
                };
                if !baseline.same_origin(&outbound) {
                    return Err(RouteFailure::ObservationChanged { search: stats });
                }
                if !admissible(&outbound, request.travel) {
                    reject(
                        &mut stats,
                        "candidate leaves travel scope or cannot stop with clearance".into(),
                    );
                    continue;
                }
                let position = outbound.frames().last().unwrap().position;
                let mut path = primitives.clone();
                path.push(primitive);
                if contains(request.goal, position) && stats.previews < request.max_previews {
                    let mut return_controls = controls.clone();
                    for primitive in path.iter().rev() {
                        primitive.reversed().append(&mut return_controls);
                    }
                    stats.previews += 1;
                    match predict(return_controls).await {
                        Ok(round_trip) => {
                            if !baseline.same_origin(&round_trip) {
                                return Err(RouteFailure::ObservationChanged { search: stats });
                            }
                            let end = round_trip.frames().last().unwrap().position;
                            if admissible(&round_trip, request.travel)
                                && (end[1] - origin[1]).abs() < 0.125
                                && (end[0] - origin[0]).hypot(end[2] - origin[2]) <= 0.35
                            {
                                stats.frontier_remaining = frontier.len();
                                return Ok(SurvivalRoute {
                                    request,
                                    outbound,
                                    round_trip,
                                    return_starts_at_tick: controls.len(),
                                    search: stats,
                                });
                            }
                            reject(
                                &mut stats,
                                "return does not reach the initial standing area with clearance"
                                    .into(),
                            );
                        }
                        Err(e) => reject(&mut stats, e.to_string()),
                    }
                }
                let key = cell(position);
                if visited
                    .get(&key)
                    .is_some_and(|cost| *cost <= controls.len())
                {
                    continue;
                }
                visited.insert(key, controls.len());
                let score = ((goal_distance(request.goal, position) + controls.len() as f64 * 0.02)
                    * 10000.0) as u64;
                let next = nodes.len();
                nodes.push(Node {
                    controls,
                    primitives: path,
                    position,
                });
                frontier.push(Reverse((score, next)));
            }
        }
    }
    stats.frontier_remaining = frontier.len();
    Err(RouteFailure::NoRouteWithinLimits { search: stats })
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxrig::checked_survival::{
        LocalPlayerState, PredictedMotionFrame, StandingContext, StandingPositionBasis,
    };
    fn request() -> RouteRequest {
        RouteRequest {
            travel: TravelBounds {
                min: [-4.0, 1.0, -4.0],
                max: [6.0, 5.0, 6.0],
            },
            goal: TravelBounds {
                min: [2.3, 1.0, 2.3],
                max: [2.7, 1.0, 2.7],
            },
            max_previews: 384,
        }
    }
    // A deterministic obstacle world checks search/order/budget semantics, not
    // Minecraft physics (covered by the real adapter and the opt-in live test).
    fn prediction(controls: Vec<SurvivalControl>, generation: u64) -> SurvivalMovementPreview {
        let initial = StandingContext {
            connection_id: 1,
            receive_sequence: 10,
            client_tick: 1,
            world_revision: 4,
            dimension: "minecraft:overworld".into(),
            position: [0.5, 1.0, 0.5],
            position_basis: StandingPositionBasis::Received {
                receive_sequence: 5,
            },
            eye_position: [0.5, 2.62, 0.5],
            bounds: [0.2, 1.0, 0.2, 0.8, 2.8, 0.8],
            on_ground: true,
            support: vec![[0, 0, 0]],
            submerged: false,
            player: LocalPlayerState::default(),
        };
        let mut position = initial.position;
        let frames = controls
            .iter()
            .enumerate()
            .map(|(tick, c)| {
                let yaw = f64::from(c.yaw).to_radians();
                let next = [
                    position[0] - yaw.sin() * f64::from(c.input.forward) * 0.125,
                    1.0,
                    position[2] + yaw.cos() * f64::from(c.input.forward) * 0.125,
                ];
                let collision =
                    next[0] >= 1.5 && next[0] <= 2.5 && next[2] >= -1.0 && next[2] < 1.2;
                if !collision {
                    position = next;
                }
                PredictedMotionFrame {
                    tick: tick as u16 + 1,
                    position,
                    velocity: [0.0; 3],
                    on_ground: true,
                    horizontal_collision: collision,
                    resting: c.input.forward == 0,
                }
            })
            .collect();
        SurvivalMovementPreview {
            initial_frame: PredictedMotionFrame {
                tick: 0,
                position: initial.position,
                velocity: [0.; 3],
                on_ground: true,
                horizontal_collision: false,
                resting: true,
            },
            initial,
            generation,
            controls,
            frames,
            terminal_clearance: TerminalClearance::Admitted {
                horizontal_margin: 0.0625,
            },
        }
    }
    #[tokio::test]
    async fn turns_around_obstruction_and_verifies_a_bounded_return() {
        let plan = search(request(), |controls| {
            std::future::ready(Ok(prediction(controls, 1)))
        })
        .await
        .unwrap();
        assert!(contains(
            plan.request.goal,
            plan.outbound.frames.last().unwrap().position
        ));
        assert!(plan.search.previews <= 384);
        assert!(plan.outbound.controls.len() <= 60);
        assert!(plan.round_trip.controls.len() <= 120);
        assert!(plan.round_trip.frames.last().unwrap().position[0].abs() < 0.85);
        assert!(!plan.return_controls().is_empty());
        assert!(
            plan.outbound
                .frames
                .iter()
                .all(|f| !(f.position[0] >= 1.5 && f.position[0] <= 2.5 && f.position[2] < 1.2))
        );
    }
    #[tokio::test]
    async fn budget_exhaustion_is_not_claimed_as_physical_impossibility() {
        let mut req = request();
        req.max_previews = 2;
        let err = search(req, |controls| {
            std::future::ready(Ok(prediction(controls, 1)))
        })
        .await
        .unwrap_err();
        let RouteFailure::NoRouteWithinLimits { search } = err else {
            panic!("{err:?}")
        };
        assert_eq!(search.previews, 2);
    }
    #[tokio::test]
    async fn changed_observation_aborts_search_and_invalid_request_never_observes() {
        let mut n = 0;
        let err = search(request(), |controls| {
            n += 1;
            std::future::ready(Ok(prediction(controls, n)))
        })
        .await
        .unwrap_err();
        assert!(matches!(err, RouteFailure::ObservationChanged { .. }));
        let mut req = request();
        req.travel.max[0] = f64::NAN;
        let err = search(
            req,
            |_| -> std::future::Ready<voxrig::Result<SurvivalMovementPreview>> {
                panic!("invalid request must not observe")
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, RouteFailure::InvalidRequest { .. }));
    }
}

#[cfg(test)]
#[path = "survival_navigation/native_trial.rs"]
mod native_trial;

#[cfg(test)]
#[path = "survival_navigation/native_access_trial.rs"]
mod native_access_trial;
