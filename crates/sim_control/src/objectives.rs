use sim_core::{AsteroidId, SiteId, StationId};

/// Objective issued by a station agent (or assignment bridge) to a ship agent.
///
/// Ship agents receive an objective and autonomously handle the tactical
/// details: transit, refueling, task execution, and deposit.
#[derive(Debug, Clone)]
pub(crate) enum ShipObjective {
    /// Mine a specific asteroid.
    Mine { asteroid_id: AsteroidId },
    /// Perform a deep scan on a specific asteroid.
    DeepScan { asteroid_id: AsteroidId },
    /// Survey a specific scan site.
    Survey { site_id: SiteId },
    /// Deposit cargo at a specific station.
    #[allow(dead_code)] // Assigned by StationAgent in VIO-449
    Deposit { station_id: StationId },
    /// No objective — ship is idle and available for assignment.
    #[allow(dead_code)] // Used by StationAgent in VIO-449
    Idle,
}
