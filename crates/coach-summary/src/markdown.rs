//! Human-readable `.md` rendering of the [`Summary`] (spec §10.3 layout).

use crate::model::Summary;
use std::fmt::Write as _;

/// Format a lap time in ms as `M:SS.cc`.
pub fn fmt_time(ms: i64) -> String {
    let neg = ms < 0;
    let ms = ms.unsigned_abs();
    let total_s = ms as f64 / 1000.0;
    let min = (total_s / 60.0).floor() as i64;
    let sec = total_s - (min as f64) * 60.0;
    format!("{}{}:{:05.2}", if neg { "-" } else { "" }, min, sec)
}

/// Format a signed delta in ms as `±S.cc s` with a faster/slower label.
fn fmt_delta(ms: i64) -> String {
    let s = ms as f64 / 1000.0;
    let label = if ms < 0 {
        "faster"
    } else if ms > 0 {
        "slower"
    } else {
        "even"
    };
    format!("{s:+.2}s, {label}")
}

/// Render the full summary as Markdown.
pub fn render_markdown(s: &Summary) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "Post-event summary — {} / {}", s.track, s.car);
    let _ = writeln!(
        o,
        "{} · {} clean laps · {} in/out laps excluded · {} outliers\n",
        s.session_type.label(),
        s.clean_lap_count,
        s.in_out_excluded,
        s.outlier_excluded
    );

    // PACE
    let p = &s.pace;
    let _ = writeln!(o, "PACE");
    let _ = writeln!(o, "  First clean lap   {}", fmt_time(p.first_ms));
    let _ = writeln!(
        o,
        "  Best lap          {}   (best − first: {})",
        fmt_time(p.best_ms),
        fmt_delta(p.best_minus_first_ms)
    );
    let _ = writeln!(
        o,
        "  Last clean lap    {}   (last − first: {})",
        fmt_time(p.last_ms),
        fmt_delta(p.last_minus_first_ms)
    );
    let _ = writeln!(o, "  Median            {}", fmt_time(p.median_ms));
    let _ = writeln!(o, "  Trend             {}", p.trend.label());
    if let Some(tb) = p.theoretical_best_ms {
        let _ = writeln!(
            o,
            "  Theoretical best  {}  (stitched segment bests)",
            fmt_time(tb)
        );
    }
    o.push('\n');

    // WHAT YOU CHANGED
    let _ = writeln!(o, "WHAT YOU CHANGED");
    if s.behavior_changes.is_empty() {
        let _ = writeln!(
            o,
            "  (no behavior metric shifted beyond its noise threshold)"
        );
    } else {
        for c in &s.behavior_changes {
            let _ = writeln!(o, "  {}", c.text);
        }
    }
    o.push('\n');

    // COACHING
    let _ = writeln!(o, "COACHING");
    let join = |v: &[crate::model::LabeledSegment]| {
        if v.is_empty() {
            "none".to_string()
        } else {
            v.iter()
                .map(|l| l.label.clone())
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    let _ = writeln!(o, "  Cued this session:  {}", join(&s.coaching.cued));
    let _ = writeln!(o, "  Fixed this session: {}", join(&s.coaching.fixed));
    o.push('\n');

    // CONSISTENCY
    let _ = writeln!(o, "CONSISTENCY");
    if let Some(lc) = &s.consistency.least_consistent {
        let _ = writeln!(
            o,
            "  Least consistent: {} (variance {:.2}s)",
            lc.label,
            lc.variance_ms / 1000.0
        );
    }
    if let Some(mc) = &s.consistency.most_consistent {
        let _ = writeln!(
            o,
            "  Most consistent:  {} (variance {:.2}s)",
            mc.label,
            mc.variance_ms / 1000.0
        );
    }

    // Race confounder caveat (spec §10.2).
    if s.race_caveat {
        o.push('\n');
        let _ = writeln!(
            o,
            "NOTE (race): absolute lap-time changes also reflect fuel burn (cars get faster\n  \
             as they lighten) and tyre wear (slower as they degrade), which this version does\n  \
             not separate from driving changes. The behavior-change section above is unaffected\n  \
             — brake points, turn-in points, and lines are driving choices independent of\n  \
             fuel/tyre."
        );
    }

    o
}
