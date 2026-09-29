use anyhow::{Result, ensure};
use std::{
    fmt::Write,
    fs::OpenOptions,
    io::{BufRead, BufReader, Seek, SeekFrom, Write as IoWrite},
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use voxrig::{BotManager, ControlState, Event, Player, Server};

struct Summary {
    corrections: u64,
    disconnects: u64,
    packets: u64,
    tick_p99: Duration,
}

struct ReportContext {
    bot_count: usize,
    requested: Duration,
    allow_server_teleports: bool,
    scenario: String,
    server_teleport_interval_secs: u64,
    pause_on_rescue_warning: bool,
    wander: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let port = env("MC_PORT", 25565_u16)?;
    let bot_count = env("BOT_COUNT", 1_usize)?;
    let duration_secs = env("DURATION_SECS", 3600_u64)?;
    let prefix = std::env::var("BOT_PREFIX").unwrap_or_else(|_| "Endurance".into());
    let output = std::env::var("REPORT_PATH").unwrap_or_else(|_| "endurance-report.md".into());
    let allow_server_teleports = env("ALLOW_SERVER_TELEPORTS", false)?;
    let scenario = std::env::var("SCENARIO").unwrap_or_else(|_| "standard".into());
    let server_teleport_interval_secs = env("SERVER_TELEPORT_INTERVAL_SECS", 0_u64)?;
    let pause_on_rescue_warning = env("PAUSE_ON_RESCUE_WARNING", false)?;
    let wander = env("WANDER", false)?;
    let diagnostic_path = std::env::var("DIAGNOSTIC_PATH").ok();
    let server_log_path = std::env::var("SERVER_LOG_PATH").ok();
    let diagnostic_lock = Arc::new(Mutex::new(()));
    if let Some(path) = diagnostic_path.as_deref() {
        prepare_output(path)?;
        std::fs::write(path, b"# movement diagnostic log\n")?;
    }
    ensure!((1..=50).contains(&bot_count), "BOT_COUNT must be 1..=50");
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    for index in 0..bot_count {
        let username = format!("{prefix}{index:02}");
        ensure!(
            username.len() <= 16,
            "generated username is longer than 16 bytes"
        );
        let bot = manager.connect(Player::offline(username)).await?;
        bot.wait_until_ready().await?;
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if bot.survival_state().await.vitals.is_some()
                    && bot.inventory().await.windows.contains_key(&0)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?;
        let initial_yaw = index as f32 * (360.0 / bot_count as f32);
        bot.look(initial_yaw, 0.0).await?;
        let control = ControlState {
            forward: true,
            sprint: index % 2 == 0,
            ..Default::default()
        };
        bot.set_control(control).await;
        if let Some(path) = diagnostic_path.clone() {
            let diagnostic_bot = bot.clone();
            let diagnostic_lock = diagnostic_lock.clone();
            let mut events = bot.subscribe();
            let attribute_bot = diagnostic_bot.clone();
            let attribute_path = path.clone();
            let attribute_lock = diagnostic_lock.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let _ = write_diagnostic(
                    &attribute_path,
                    &attribute_lock,
                    &attribute_bot,
                    "sprint_attribute_probe",
                )
                .await;
            });
            tokio::spawn(async move {
                while let Ok(event) = events.recv().await {
                    if matches!(
                        event,
                        Event::PositionCorrection(_) | Event::Disconnected { .. }
                    ) {
                        let _ = write_diagnostic(
                            &path,
                            &diagnostic_lock,
                            &diagnostic_bot,
                            &format!("{event:?}"),
                        )
                        .await;
                    }
                }
            });
        }
        if wander {
            tokio::spawn(wander_loop(bot.clone(), index, initial_yaw, control));
        }
        if pause_on_rescue_warning {
            let controlled_bot = bot.clone();
            let mut events = bot.subscribe();
            tokio::spawn(async move {
                while let Ok(event) = events.recv().await {
                    if let Event::Chat(chat) = event {
                        if chat.json.contains("MC_AI_RESCUE_PREPARE") {
                            controlled_bot.suspend_movement_for(Duration::from_secs(7));
                        }
                    }
                }
            });
        }
    }
    if let (Some(path), Some(server_log_path)) = (diagnostic_path.clone(), server_log_path) {
        let diagnostic_manager = manager.clone();
        let diagnostic_lock = diagnostic_lock.clone();
        tokio::spawn(async move {
            let _ = watch_server_log(
                &server_log_path,
                &path,
                &diagnostic_lock,
                &diagnostic_manager,
            )
            .await;
        });
    }
    prepare_output(&output)?;
    let started = Instant::now();
    let requested = Duration::from_secs(duration_secs);
    let report_context = ReportContext {
        bot_count,
        requested,
        allow_server_teleports,
        scenario,
        server_teleport_interval_secs,
        pause_on_rescue_warning,
        wander,
    };
    let mut summary =
        write_report(&manager, &output, started.elapsed(), &report_context, false).await?;
    while started.elapsed() < requested {
        tokio::time::sleep((requested - started.elapsed()).min(Duration::from_secs(10))).await;
        summary = write_report(
            &manager,
            &output,
            started.elapsed().min(requested),
            &report_context,
            started.elapsed() >= requested,
        )
        .await?;
    }
    manager.disconnect_all().await?;
    ensure!(
        manager.usernames().await.is_empty(),
        "BotManager retained bots after disconnect_all"
    );
    ensure!(
        summary.disconnects == 0,
        "endurance run had {} disconnects",
        summary.disconnects
    );
    if !allow_server_teleports {
        ensure!(
            summary.corrections == 0,
            "endurance run had {} corrections",
            summary.corrections
        );
    }
    ensure!(
        summary.tick_p99 < Duration::from_millis(5),
        "physics p99 was {:?}",
        summary.tick_p99
    );
    println!(
        "wrote {output}: bots={bot_count}, packets={}, p99={:?}",
        summary.packets, summary.tick_p99
    );
    Ok(())
}

async fn write_report(
    manager: &BotManager,
    output: &str,
    elapsed: Duration,
    context: &ReportContext,
    complete: bool,
) -> Result<Summary> {
    let metrics = manager.physics_metrics().await;
    let corrections: u64 = metrics.values().map(|m| m.position_corrections).sum();
    let server_position_packets: u64 = metrics.values().map(|m| m.server_position_packets).sum();
    let total_correction_distance: f64 =
        metrics.values().map(|m| m.total_correction_distance).sum();
    let largest_correction_distance = metrics
        .values()
        .map(|m| m.largest_correction_distance)
        .fold(0.0_f64, f64::max);
    let disconnects: u64 = metrics.values().map(|m| m.disconnects).sum();
    let packets: u64 = metrics.values().map(|m| m.movement_packets).sum();
    let tick_p99 = metrics
        .values()
        .map(|m| m.physics_tick_p99)
        .max()
        .unwrap_or_default();
    let tick_max = metrics
        .values()
        .map(|m| m.physics_tick_max)
        .max()
        .unwrap_or_default();
    let lag_p99 = metrics
        .values()
        .map(|m| m.physics_tick_lag_p99)
        .max()
        .unwrap_or_default();
    let lag_max = metrics
        .values()
        .map(|m| m.physics_tick_lag_max)
        .max()
        .unwrap_or_default();
    let rss = process_rss_kib().unwrap_or(0);
    let mut report = String::new();
    writeln!(report, "# Endurance test result\n")?;
    writeln!(
        report,
        "- Status: {}",
        if complete { "complete" } else { "running" }
    )?;
    writeln!(report, "- Bots: {}", context.bot_count)?;
    writeln!(report, "- Scenario: {}", context.scenario)?;
    writeln!(
        report,
        "- Server teleports allowed: {}",
        context.allow_server_teleports
    )?;
    if context.server_teleport_interval_secs > 0 {
        writeln!(
            report,
            "- Server teleport interval: {} seconds",
            context.server_teleport_interval_secs
        )?;
    }
    writeln!(
        report,
        "- Pause on rescue warning: {}",
        context.pause_on_rescue_warning
    )?;
    writeln!(report, "- Wander input generator: {}", context.wander)?;
    writeln!(
        report,
        "- Requested duration: {} seconds",
        context.requested.as_secs()
    )?;
    writeln!(
        report,
        "- Recorded duration: {:.3} seconds",
        elapsed.as_secs_f64()
    )?;
    writeln!(report, "- Movement packets: {packets}")?;
    writeln!(
        report,
        "- Server position packets: {server_position_packets}"
    )?;
    writeln!(report, "- Position corrections: {corrections}")?;
    writeln!(
        report,
        "- Total correction distance: {total_correction_distance:.9}"
    )?;
    writeln!(
        report,
        "- Largest correction distance: {largest_correction_distance:.9}"
    )?;
    writeln!(report, "- Disconnects: {disconnects}")?;
    writeln!(report, "- Worst per-Bot physics tick p99: {tick_p99:?}")?;
    writeln!(report, "- Maximum physics tick: {tick_max:?}")?;
    writeln!(report, "- Worst per-Bot queue lag p99: {lag_p99:?}")?;
    writeln!(report, "- Maximum queue lag: {lag_max:?}")?;
    writeln!(report, "- Process RSS: {rss} KiB")?;
    atomic_write(output, report.as_bytes())?;
    Ok(Summary {
        corrections,
        disconnects,
        packets,
        tick_p99,
    })
}

async fn write_diagnostic(
    path: &str,
    lock: &Mutex<()>,
    bot: &voxrig::Bot,
    event: &str,
) -> Result<()> {
    let player = bot.player().await;
    let motion = bot.motion().await;
    let control = bot.control().await;
    let survival = bot.survival_state().await;
    let movement_attribute = survival
        .attributes
        .get("minecraft:generic.movement_speed")
        .or_else(|| survival.attributes.get("generic.movement_speed"));
    let blocks = bot
        .observe(2)
        .await?
        .into_iter()
        .filter_map(|block| {
            let state = block.state_id?;
            let name = voxrig::block_name_from_state(state)?;
            (name != "air").then_some(format!(
                "{}@{},{},{}#{}",
                name, block.x, block.y, block.z, state
            ))
        })
        .collect::<Vec<_>>();
    let line = format!(
        "t={:?} bot={} event={} player={:?} motion={:?} control={:?} movement_attribute={:?} blocks=[{}]\n",
        std::time::SystemTime::now(),
        player.username,
        event,
        player,
        motion,
        control,
        movement_attribute,
        blocks.join(",")
    );
    let _guard = lock.lock().await;
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(line.as_bytes())?;
    Ok(())
}

async fn watch_server_log(
    server_log_path: &str,
    diagnostic_path: &str,
    lock: &Mutex<()>,
    manager: &BotManager,
) -> Result<()> {
    let file = std::fs::File::open(server_log_path)?;
    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::End(0))?;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            continue;
        }
        let Some(message) = line.split("]: ").nth(1) else {
            continue;
        };
        let warning = [
            " moved wrongly!",
            " moved too quickly!",
            " was kicked for floating",
        ]
        .into_iter()
        .find(|warning| message.contains(warning));
        let Some(warning) = warning else {
            continue;
        };
        let username = message.split(warning).next().unwrap_or_default();
        if let Some(bot) = manager.get(username).await {
            write_diagnostic(
                diagnostic_path,
                lock,
                &bot,
                &format!("server_log={}", message.trim()),
            )
            .await?;
        }
    }
}

async fn wander_loop(bot: voxrig::Bot, index: usize, mut yaw: f32, mut control: ControlState) {
    let mut ticker = tokio::time::interval(Duration::from_millis(100));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last = bot.player().await;
    let mut stationary_ticks = 0_u32;
    let mut ticks_until_turn = 40_u32 + (index as u32 % 30);
    let mut jump_phase = 0_u32;
    let mut seed = (index as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15);

    loop {
        ticker.tick().await;
        let player = bot.player().await;
        let dx = player.x - last.x;
        let dy = player.y - last.y;
        let dz = player.z - last.z;
        if dx * dx + dy * dy + dz * dz < 0.0025 {
            stationary_ticks += 1;
        } else {
            stationary_ticks = 0;
        }
        last = player.clone();

        ticks_until_turn = ticks_until_turn.saturating_sub(1);
        let stuck = stationary_ticks >= 30;
        if stuck || ticks_until_turn == 0 {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let turn = if stuck {
                120.0 + (seed % 121) as f32
            } else {
                45.0 + (seed % 136) as f32
            };
            yaw = (yaw + turn).rem_euclid(360.0);
            if bot.look(yaw, 0.0).await.is_err() {
                break;
            }
            stationary_ticks = 0;
            ticks_until_turn = 40 + ((seed >> 16) % 41) as u32;
        }

        control.jump = jump_phase < 4;
        jump_phase = (jump_phase + 1) % 5;
        bot.set_control(control).await;
    }
}

fn prepare_output(output: &str) -> Result<()> {
    if let Some(parent) = Path::new(output).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

fn atomic_write(output: &str, bytes: &[u8]) -> Result<()> {
    let temporary = format!("{output}.tmp");
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(temporary, output)?;
    Ok(())
}

fn env<T: std::str::FromStr>(name: &str, default: T) -> Result<T>
where
    T::Err: std::error::Error + Send + Sync + 'static,
{
    Ok(match std::env::var(name) {
        Ok(value) => value.parse()?,
        Err(_) => default,
    })
}

fn process_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}
