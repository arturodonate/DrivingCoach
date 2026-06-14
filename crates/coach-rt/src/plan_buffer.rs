//! Double-buffered plan handover with the deadline fallback (spec §9.4).

use coach_core::cue::CoachingPlan;
use std::sync::Arc;

/// Holds the live plan and an optional staged next-lap plan. At the lap boundary
/// the caller swaps; if no next plan is staged in time, the current (one-lap-
/// stale) plan keeps running — correct-enough coaching beats a late/absent plan
/// (spec §9.4).
pub struct PlanBuffer {
    current: Arc<CoachingPlan>,
    pending: Option<Arc<CoachingPlan>>,
    /// Counts boundaries where the fallback engaged (should be ~never; spec §9.4).
    pub fallback_count: u64,
}

impl PlanBuffer {
    pub fn new(initial: Arc<CoachingPlan>) -> Self {
        Self {
            current: initial,
            pending: None,
            fallback_count: 0,
        }
    }

    /// Stage the next lap's freshly compiled plan.
    pub fn stage(&mut self, plan: Arc<CoachingPlan>) {
        self.pending = Some(plan);
    }

    /// At the lap boundary: promote the staged plan if present, else keep the
    /// current plan and record a fallback. Returns the plan now in force.
    pub fn swap_at_boundary(&mut self) -> Arc<CoachingPlan> {
        match self.pending.take() {
            Some(next) => self.current = next,
            None => self.fallback_count += 1,
        }
        self.current.clone()
    }

    pub fn current(&self) -> Arc<CoachingPlan> {
        self.current.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(id: &str) -> Arc<CoachingPlan> {
        Arc::new(CoachingPlan {
            track_id: id.into(),
            car_id: "c".into(),
            source_lap_id: None,
            cues: vec![],
        })
    }

    #[test]
    fn promotes_staged_plan_else_falls_back() {
        let mut b = PlanBuffer::new(plan("lap0"));
        b.stage(plan("lap1"));
        assert_eq!(b.swap_at_boundary().track_id.as_str(), "lap1");
        // No plan staged for the next boundary → fallback keeps lap1.
        assert_eq!(b.swap_at_boundary().track_id.as_str(), "lap1");
        assert_eq!(b.fallback_count, 1);
    }
}
