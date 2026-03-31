use sim_core::{inventory_volume_m3, Command, CommandEnvelope, InventoryItem};

use crate::behaviors::make_cmd;

use super::super::{StationConcern, StationContext};

/// 6. Jettison slag when storage usage exceeds threshold.
pub(in crate::agents) struct SlagJettison;

impl StationConcern for SlagJettison {
    fn name(&self) -> &'static str {
        "slag_jettison"
    }
    fn should_run(&self, _ctx: &StationContext) -> bool {
        true
    }
    fn generate(&mut self, ctx: &mut StationContext) -> Vec<CommandEnvelope> {
        let Some(station) = ctx.state.stations.get(ctx.station_id) else {
            return Vec::new();
        };

        let threshold = ctx.content.autopilot.slag_jettison_pct;
        if station.cargo_capacity_m3 <= 0.0 {
            return Vec::new();
        }
        let used_m3 = inventory_volume_m3(&station.inventory, ctx.content);
        let used_pct = used_m3 / station.cargo_capacity_m3;

        if used_pct >= threshold
            && station
                .inventory
                .iter()
                .any(|i| matches!(i, InventoryItem::Slag { .. }))
        {
            vec![make_cmd(
                ctx.owner,
                ctx.state.meta.tick,
                ctx.next_id,
                Command::JettisonSlag {
                    station_id: ctx.station_id.clone(),
                },
            )]
        } else {
            Vec::new()
        }
    }
}
