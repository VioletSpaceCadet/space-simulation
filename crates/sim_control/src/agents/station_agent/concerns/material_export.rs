use sim_core::{trade, Command, CommandEnvelope};

use crate::behaviors::{build_export_candidates, make_cmd};

use super::super::{StationConcern, StationContext};

/// 7. Export surplus materials for revenue.
pub(in crate::agents) struct MaterialExport;

impl StationConcern for MaterialExport {
    fn name(&self) -> &'static str {
        "material_export"
    }
    fn should_run(&self, ctx: &StationContext) -> bool {
        ctx.trade_export_unlocked
    }
    fn generate(&mut self, ctx: &mut StationContext) -> Vec<CommandEnvelope> {
        let Some(station) = ctx.state.stations.get(ctx.station_id) else {
            return Vec::new();
        };

        let batch_size_kg = ctx.content.autopilot.export_batch_size_kg;
        let min_revenue = ctx.content.autopilot.export_min_revenue;
        let mut commands = Vec::new();

        let candidates = build_export_candidates(station, &ctx.content.autopilot, batch_size_kg);
        for candidate in candidates {
            if trade::compute_export_revenue(&candidate, &ctx.content.pricing, ctx.content)
                .is_none_or(|rev| rev < min_revenue)
            {
                continue;
            }
            if !trade::has_enough_for_export(&station.core.inventory, &candidate) {
                continue;
            }
            commands.push(make_cmd(
                ctx.owner,
                ctx.state.meta.tick,
                ctx.next_id,
                Command::Export {
                    station_id: ctx.station_id.clone(),
                    item_spec: candidate,
                },
            ));
        }

        commands
    }
}
