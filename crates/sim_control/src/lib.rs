use sim_core::{
    AnomalyTag, AsteroidId, Command, CommandEnvelope, CommandId, GameContent, GameState,
    PrincipalId, SiteId, TaskKind, TechId,
};

pub trait CommandSource {
    fn generate_commands(
        &mut self,
        state: &GameState,
        content: &GameContent,
        next_command_id: &mut u64,
    ) -> Vec<CommandEnvelope>;
}

/// Drives ships automatically:
/// 1. Survey unscanned sites in order.
/// 2. Once all sites are surveyed and deep scan is unlocked, deep-scan
///    IronRich asteroids whose composition is still unknown.
pub struct AutopilotController;

const AUTOPILOT_OWNER: &str = "principal_autopilot";
const IRON_RICH_CONFIDENCE_THRESHOLD: f32 = 0.7;

impl CommandSource for AutopilotController {
    fn generate_commands(
        &mut self,
        state: &GameState,
        _content: &GameContent,
        next_command_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        let owner = PrincipalId(AUTOPILOT_OWNER.to_string());

        // Collect idle ships owned by autopilot, sorted for determinism.
        let mut idle_ships: Vec<_> = state
            .ships
            .values()
            .filter(|ship| {
                ship.owner == owner
                    && ship.task.as_ref().map_or(true, |t| matches!(t.kind, TaskKind::Idle))
            })
            .map(|ship| ship.id.clone())
            .collect();
        idle_ships.sort_by(|a, b| a.0.cmp(&b.0));

        let deep_scan_unlocked =
            state.research.unlocked.contains(&TechId("tech_deep_scan_v1".to_string()));

        // Deep scan candidates: IronRich confidence above threshold, composition unknown.
        let mut deep_scan_candidates: Vec<AsteroidId> = state
            .asteroids
            .values()
            .filter(|asteroid| {
                asteroid.knowledge.composition.is_none()
                    && asteroid.knowledge.tag_beliefs.iter().any(|(tag, conf)| {
                        *tag == AnomalyTag::IronRich && *conf > IRON_RICH_CONFIDENCE_THRESHOLD
                    })
            })
            .map(|a| a.id.clone())
            .collect();
        deep_scan_candidates.sort_by(|a, b| a.0.cmp(&b.0));

        let mut commands = Vec::new();
        let mut next_site = state.scan_sites.iter();
        let mut next_deep_scan = deep_scan_candidates.iter();

        for ship_id in idle_ships {
            let ship = &state.ships[&ship_id];
            let task_kind = if let Some(site) = next_site.next() {
                TaskKind::Survey { site: SiteId(site.id.0.clone()) }
            } else if deep_scan_unlocked {
                match next_deep_scan.next() {
                    Some(asteroid_id) => TaskKind::DeepScan { asteroid: asteroid_id.clone() },
                    None => continue, // nothing to do
                }
            } else {
                continue; // no sites left and tech not unlocked yet — wait
            };

            let cmd_id = CommandId(format!("cmd_{:06}", *next_command_id));
            *next_command_id += 1;

            commands.push(CommandEnvelope {
                id: cmd_id,
                issued_by: ship.owner.clone(),
                issued_tick: state.meta.tick,
                execute_at_tick: state.meta.tick,
                command: Command::AssignShipTask { ship_id, task_kind },
            });
        }

        commands
    }
}

/// Replays a scripted sequence of commands from a JSON file.
pub struct ScenarioSource {
    // TODO: load tick -> Vec<Command> map from file
}

impl CommandSource for ScenarioSource {
    fn generate_commands(
        &mut self,
        _state: &GameState,
        _content: &GameContent,
        _next_command_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        // TODO: emit commands scheduled for the current tick
        vec![]
    }
}
