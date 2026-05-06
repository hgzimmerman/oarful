//! Tooltip and short-format helpers for timeline items.

use lineup_db::timeline::{Blade, Block, Group, Segment, Slide};

pub(super) fn format_duration_short(minutes: f64) -> String {
    if minutes >= 1.0 {
        format!("{:.0}'", minutes)
    } else {
        format!("{:.1}'", minutes)
    }
}

pub(super) fn block_tooltip(b: &Block, is_dock: bool, slack: f64) -> String {
    if is_dock {
        return if slack > 0.0 {
            format!("Dock — {:.0} min slack", slack)
        } else {
            format!("Dock — {:.0} min over target", -slack)
        };
    }
    let mut parts = vec![b.block_type.label().to_string(), b.duration.display()];
    if !b.note.is_empty() {
        parts.push(format!("— {}", b.note));
    }
    parts.join(" · ")
}

pub(super) fn segment_tooltip(s: &Segment) -> String {
    let mut parts = vec![s.seg_type.label().to_string(), s.duration.display()];
    if let Some([lo, hi]) = s.rate {
        if lo == hi {
            parts.push(format!("r{lo}"));
        } else {
            parts.push(format!("r{lo}-{hi}"));
        }
    }
    if let Some(int) = s.intensity {
        parts.push(format!("@ {}", int.full_name()));
    }
    if let Some(sl) = s.partial {
        if sl != Slide::Full {
            parts.push(sl.label().to_string());
        }
    }
    if !s.pause.is_empty() {
        let labels: Vec<&str> = s.pause.iter().map(|p| p.label()).collect();
        parts.push(format!("pause @ {}", labels.join(" + ")));
    }
    match s.blade {
        Some(Blade::Square) => parts.push("on square".to_string()),
        Some(Blade::PartialFeather) => parts.push("partial feather".to_string()),
        _ => {}
    }
    for hd in &s.drills {
        parts.push(hd.label().to_string());
    }
    if !s.note.is_empty() {
        parts.push(format!("— {}", s.note));
    }
    parts.join(" · ")
}

pub(super) fn group_tooltip(g: &Group) -> String {
    let mut parts = vec![
        format!(
            "{}: {}",
            g.group_type.label(),
            if g.name.is_empty() {
                "(unnamed)"
            } else {
                &g.name
            }
        ),
        format!("{} segments", g.segments.len()),
        format!("{:.0} min", g.approx_minutes()),
    ];
    if g.rotation.is_active() {
        parts.push(g.rotation.label());
    }
    if !g.note.is_empty() {
        parts.push(format!("— {}", g.note));
    }
    parts.join(" · ")
}
