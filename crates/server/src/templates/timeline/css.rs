//! CSS helper functions for timeline rendering.

use lineup_db::timeline::{BlockType, GroupType, SegmentType};

pub(super) fn block_type_css(bt: BlockType) -> &'static str {
    crate::templates::history::block_type_css(bt)
}

pub(crate) fn group_type_css(gt: GroupType) -> &'static str {
    match gt {
        GroupType::Warmup => "color: var(--good); background: color-mix(in oklch, var(--good) 12%, var(--paper)); border-color: color-mix(in oklch, var(--good) 28%, var(--rule))",
        GroupType::Piece => "color: var(--accent); background: color-mix(in oklch, var(--accent) 12%, var(--paper)); border-color: color-mix(in oklch, var(--accent) 28%, var(--rule))",
    }
}

pub(crate) fn seg_type_css(st: SegmentType) -> &'static str {
    match st {
        SegmentType::Work => "color: var(--ink-2); background: var(--paper-2); border-color: var(--rule)",
        SegmentType::Rest => "color: var(--muted); background: var(--paper-2); border-color: var(--rule)",
        SegmentType::Turn => "color: var(--warn); background: color-mix(in oklch, var(--warn) 12%, var(--paper)); border-color: color-mix(in oklch, var(--warn) 28%, var(--rule))",
    }
}

pub(crate) fn strip_bg_block(bt: BlockType) -> &'static str {
    match bt {
        BlockType::Launch | BlockType::Dock => {
            "background: color-mix(in oklch, var(--cox) 18%, var(--paper))"
        }
        BlockType::Rest => "background: var(--paper-2)",
        BlockType::Turn => "background: color-mix(in oklch, var(--warn) 18%, var(--paper))",
    }
}

pub(crate) fn strip_bg_group(gt: GroupType) -> &'static str {
    match gt {
        GroupType::Warmup => "background: color-mix(in oklch, var(--good) 12%, var(--paper))",
        GroupType::Piece => "background: color-mix(in oklch, var(--accent) 12%, var(--paper))",
    }
}

pub(crate) fn strip_bg_seg(st: SegmentType, gt: GroupType) -> &'static str {
    match st {
        SegmentType::Work => match gt {
            GroupType::Warmup => "background: color-mix(in oklch, var(--good) 22%, var(--paper))",
            GroupType::Piece => "background: color-mix(in oklch, var(--accent) 22%, var(--paper))",
        },
        SegmentType::Rest => "background: var(--paper-3)",
        SegmentType::Turn => "background: color-mix(in oklch, var(--warn) 18%, var(--paper))",
    }
}

pub(crate) fn chip_style(active: bool) -> &'static str {
    if active {
        "color: var(--ink); background: var(--paper-2); border-color: var(--ink-3)"
    } else {
        "color: var(--muted); border-color: var(--rule); background: var(--paper)"
    }
}

pub(super) fn slide_value(s: lineup_db::timeline::Slide) -> &'static str {
    use lineup_db::timeline::Slide;
    match s {
        Slide::Full => "full",
        Slide::ArmsOnly => "arms-only",
        Slide::ArmsBody => "arms-body",
        Slide::Quarter => "quarter",
        Slide::Half => "half",
        Slide::ThreeQuarter => "three-quarter",
        Slide::FullLegs => "full-legs",
        Slide::LegsBody => "legs-body",
    }
}

pub(super) fn pause_value(p: lineup_db::timeline::PausePoint) -> &'static str {
    use lineup_db::timeline::PausePoint;
    match p {
        PausePoint::Release => "release",
        PausePoint::ArmsAway => "arms-away",
        PausePoint::BodiesOver => "bodies-over",
        PausePoint::ThreeQuarter => "three-quarter",
        PausePoint::Half => "half",
        PausePoint::Quarter => "quarter",
        PausePoint::Catch => "catch",
    }
}

pub(super) fn hand_drill_value(h: lineup_db::timeline::HandDrill) -> &'static str {
    use lineup_db::timeline::HandDrill;
    match h {
        HandDrill::FeetOut => "feet-out",
        HandDrill::InsideArm => "inside-arm",
        HandDrill::OutsideArm => "outside-arm",
        HandDrill::InsideLeg => "inside-leg",
        HandDrill::OutsideLeg => "outside-leg",
        HandDrill::CutTheCake => "cut-the-cake",
        HandDrill::GunnelTaps => "gunnel-taps",
        HandDrill::WideGrip => "wide-grip",
        HandDrill::SlapCatches => "slap-catches",
    }
}
