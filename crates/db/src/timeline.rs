//! Practice timeline — structured plan for a rowing practice.
//!
//! A timeline is an ordered list of items: bare blocks and groups.
//!
//! **Bare blocks** (top-level only): Launch, Rest, Turn, Dock.
//! **Groups** (warmup or piece): always contain ≥1 segment.
//!   Segments inside groups are Work, Rest, or Turn.
//!
//! Structural invariants:
//! - The first item must be a Launch block.
//! - The last item must be a Dock block.
//! - Launch and Dock cannot be added, removed, or reordered.
//! - Warmup and Piece always exist as groups, never bare blocks.

use serde::{Deserialize, Serialize};

/// Top-level timeline document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Timeline {
    pub target_minutes: u32,
    pub items: Vec<TimelineItem>,
}

/// Either a bare block or a group (warmup/piece).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind")]
pub enum TimelineItem {
    #[serde(rename = "block")]
    Block(Block),
    #[serde(rename = "group")]
    Group(Group),
}

/// A bare top-level block: Launch, Rest, Turn, or Dock.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Block {
    pub id: String,
    #[serde(rename = "type")]
    pub block_type: BlockType,
    pub duration: Duration,
    #[serde(default)]
    pub note: String,
}

/// Bare block types (top-level only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockType {
    Launch,
    Rest,
    Turn,
    Dock,
}

impl BlockType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Launch => "Launch",
            Self::Rest => "Rest",
            Self::Turn => "Turn",
            Self::Dock => "Dock",
        }
    }

    pub fn is_structural(self) -> bool {
        matches!(self, Self::Launch | Self::Dock)
    }

    /// Bare block types the user can add from the palette.
    pub const USER_ADDABLE: &'static [Self] = &[Self::Rest, Self::Turn];
}

impl std::fmt::Display for BlockType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// A group is always a Warmup or Piece, containing segments.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Group {
    pub id: String,
    pub group_type: GroupType,
    pub name: String,
    pub segments: Vec<Segment>,
    /// Repeat the group N times. None or Some(1) = no repeat.
    #[serde(default)]
    pub repeat: Option<u8>,
    #[serde(default)]
    pub rotation: Rotation,
    #[serde(default)]
    pub note: String,
}

/// Whether a group represents a warmup or a piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GroupType {
    Warmup,
    Piece,
}

impl GroupType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Warmup => "Warmup",
            Self::Piece => "Piece",
        }
    }
}

impl std::fmt::Display for GroupType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// A segment inside a warmup/piece group.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Segment {
    pub id: String,
    #[serde(rename = "type")]
    pub seg_type: SegmentType,
    pub duration: Duration,
    /// Stroke rate `[low, high]`.  Same value = fixed rate.
    #[serde(default)]
    pub rate: Option<[u8; 2]>,
    #[serde(default)]
    pub intensity: Option<Intensity>,
    /// Partial stroke modifier (was "slide").
    #[serde(default, alias = "slide")]
    pub partial: Option<Slide>,
    /// Pause points in the recovery — multi-select.
    #[serde(default)]
    pub pause: Vec<PausePoint>,
    /// Pause frequency: every N strokes. None = every stroke.
    #[serde(default)]
    pub pause_every: Option<u32>,
    #[serde(default)]
    pub blade: Option<Blade>,
    #[serde(default, alias = "hand_drills")]
    pub drills: Vec<HandDrill>,
    #[serde(default)]
    pub note: String,
}

/// Segment types inside a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SegmentType {
    Work,
    Rest,
    Turn,
}

impl SegmentType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Work => "Work",
            Self::Rest => "Rest",
            Self::Turn => "Turn",
        }
    }

    pub fn is_work(self) -> bool {
        self == Self::Work
    }
}

impl std::fmt::Display for SegmentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

// ── Drill / modifier enums (unchanged) ──────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Intensity {
    Paddle,
    #[serde(rename = "ut2")]
    Ut2,
    #[serde(rename = "ut1")]
    Ut1,
    #[serde(rename = "at")]
    At,
    #[serde(rename = "tr")]
    Tr,
    #[serde(rename = "an")]
    An,
}

impl Intensity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Paddle => "Paddle",
            Self::Ut2 => "UT2",
            Self::Ut1 => "UT1",
            Self::At => "AT",
            Self::Tr => "TR",
            Self::An => "AN",
        }
    }

    pub fn full_name(self) -> &'static str {
        match self {
            Self::Paddle => "Easy, conversational pace",
            Self::Ut2 => "Utilization Training 2",
            Self::Ut1 => "Utilization Training 1",
            Self::At => "Anaerobic Threshold",
            Self::Tr => "Transport Training",
            Self::An => "Anaerobic Training",
        }
    }

    pub const ALL: &'static [Self] = &[
        Self::Paddle,
        Self::Ut2,
        Self::Ut1,
        Self::At,
        Self::Tr,
        Self::An,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Slide {
    Full,
    ArmsOnly,
    ArmsBody,
    Quarter,
    Half,
    ThreeQuarter,
    FullLegs,
    LegsBody,
}

impl Slide {
    pub fn label(self) -> &'static str {
        match self {
            Self::Full => "full stroke",
            Self::ArmsOnly => "arms only",
            Self::ArmsBody => "arms + body",
            Self::Quarter => "\u{00bc} slide",
            Self::Half => "\u{00bd} slide",
            Self::ThreeQuarter => "\u{00be} slide",
            Self::FullLegs => "full legs",
            Self::LegsBody => "legs + body",
        }
    }

    pub const ALL: &'static [Self] = &[
        Self::Full,
        Self::ArmsOnly,
        Self::ArmsBody,
        Self::Quarter,
        Self::Half,
        Self::ThreeQuarter,
        Self::FullLegs,
        Self::LegsBody,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PausePoint {
    Release,
    ArmsAway,
    BodiesOver,
    ThreeQuarter,
    Half,
    Quarter,
    Catch,
}

impl PausePoint {
    pub fn label(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::ArmsAway => "arms away",
            Self::BodiesOver => "bodies over",
            Self::ThreeQuarter => "\u{00be} slide",
            Self::Half => "\u{00bd} slide",
            Self::Quarter => "\u{00bc} slide",
            Self::Catch => "catch",
        }
    }

    pub const ALL: &'static [Self] = &[
        Self::Release,
        Self::ArmsAway,
        Self::BodiesOver,
        Self::ThreeQuarter,
        Self::Half,
        Self::Quarter,
        Self::Catch,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Blade {
    Feather,
    Square,
    #[serde(rename = "partial-feather")]
    PartialFeather,
}

impl Blade {
    pub fn label(self) -> &'static str {
        match self {
            Self::Feather => "feather",
            Self::Square => "on square",
            Self::PartialFeather => "partial feather",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HandDrill {
    FeetOut,
    InsideArm,
    OutsideArm,
    InsideLeg,
    OutsideLeg,
    CutTheCake,
    GunnelTaps,
    WideGrip,
    SlapCatches,
}

impl HandDrill {
    pub fn label(self) -> &'static str {
        match self {
            Self::FeetOut => "feet out",
            Self::InsideArm => "inside arm",
            Self::OutsideArm => "outside arm",
            Self::InsideLeg => "inside leg",
            Self::OutsideLeg => "outside leg",
            Self::CutTheCake => "cut the cake",
            Self::GunnelTaps => "gunnel taps",
            Self::WideGrip => "wide grip",
            Self::SlapCatches => "slap catches",
        }
    }

    pub const ALL: &'static [Self] = &[
        Self::FeetOut,
        Self::InsideArm,
        Self::OutsideArm,
        Self::InsideLeg,
        Self::OutsideLeg,
        Self::CutTheCake,
        Self::GunnelTaps,
        Self::WideGrip,
        Self::SlapCatches,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DurationUnit {
    Min,
    Meters,
    Strokes,
}

impl DurationUnit {
    pub fn label(self) -> &'static str {
        match self {
            Self::Min => "min",
            Self::Meters => "m",
            Self::Strokes => "str",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Duration {
    pub value: f64,
    pub unit: DurationUnit,
}

/// How many rowers row at once, and how the window slides.
///
/// Example: "row by 6, rotate by 2" in an 8+ means 2 sit out, the
/// window slides by 2 seats each rotation = 4 rotations (chunks).
///
/// "row by 4, rotate by 2" in an 8+ means 4 sit out, slide by 2 =
/// 4 rotations (overlapping halves).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rotation {
    /// How many seats row at once. None = everyone.
    #[serde(default)]
    pub row_by: Option<u8>,
    /// Slide the window by N seats each rotation. None = no rotation.
    #[serde(default)]
    pub rotate_by: Option<u8>,
    /// When to rotate: per segment, per group, or every N.
    #[serde(default)]
    pub rotate_per: RotatePer,
    /// How many rotations for full boat coverage (for `PerGroup` and
    /// `PerSegment` modes). Since plans are boat-agnostic, the coach
    /// sets this. Default: 2 (typical for "by halves").
    #[serde(default)]
    pub rotations: Option<u8>,
}

/// When the seat rotation happens.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum RotatePer {
    /// No rotation — everyone rows.
    #[default]
    #[serde(rename = "none")]
    None,
    /// Rotate after each segment within the group.
    #[serde(rename = "segment")]
    Segment,
    /// Repeat the entire group once per seat group.
    #[serde(rename = "group")]
    Group,
    /// Rotate within each segment every N strokes/minutes.
    #[serde(rename = "every")]
    Every { value: f64, unit: DurationUnit },
}

impl Default for Rotation {
    fn default() -> Self {
        Self {
            row_by: None,
            rotate_by: None,
            rotate_per: RotatePer::None,
            rotations: None,
        }
    }
}

impl Rotation {
    /// Whether any rotation is configured.
    pub fn is_active(&self) -> bool {
        self.row_by.is_some() && self.rotate_by.is_some()
    }

    /// Human-readable label, e.g. "row by 6, rotate by 2, per segment".
    pub fn label(&self) -> String {
        if !self.is_active() {
            return "all row".to_string();
        }
        let mut parts = Vec::new();
        if let Some(r) = self.row_by {
            parts.push(format!("row by {r}"));
        }
        if let Some(r) = self.rotate_by {
            parts.push(format!("rotate by {r}"));
        }
        if let Some(n) = self.rotations {
            parts.push(format!("{n}×"));
        }
        match &self.rotate_per {
            RotatePer::None => {}
            RotatePer::Segment => parts.push("per segment".to_string()),
            RotatePer::Group => parts.push("per group".to_string()),
            RotatePer::Every { value, unit } => {
                let d = Duration {
                    value: *value,
                    unit: *unit,
                };
                parts.push(format!("every {}", d.display()));
            }
        }
        parts.join(", ")
    }
}

// ── Duration math ────────────────────────────────────────────────────

impl Duration {
    pub fn approx_minutes(self) -> f64 {
        match self.unit {
            DurationUnit::Min => self.value,
            DurationUnit::Meters => self.value / 250.0,
            DurationUnit::Strokes => self.value / 25.0,
        }
    }

    pub fn display(self) -> String {
        match self.unit {
            DurationUnit::Min => format!("{}'", self.value),
            DurationUnit::Meters => format!("{}m", self.value),
            DurationUnit::Strokes => format!("{} str", self.value),
        }
    }
}

impl Block {
    pub fn approx_minutes(&self) -> f64 {
        self.duration.approx_minutes()
    }
}

impl Segment {
    pub fn approx_minutes(&self) -> f64 {
        self.duration.approx_minutes()
    }
}

impl Group {
    pub fn approx_minutes(&self) -> f64 {
        let base: f64 = self.segments.iter().map(|s| s.approx_minutes()).sum();
        let repeat = self.repeat.unwrap_or(1).max(1) as f64;
        let rotation = match self.rotation.rotate_per {
            RotatePer::Group => self.rotation.rotations.unwrap_or(1) as f64,
            _ => 1.0,
        };
        base * repeat * rotation
    }

    /// Total number of times the segment sequence appears in the
    /// timeline strip (repeat × rotation multiplier).
    pub fn strip_repetitions(&self) -> usize {
        let repeat = self.repeat.unwrap_or(1).max(1) as usize;
        let rotation = match self.rotation.rotate_per {
            RotatePer::Group => self.rotation.rotations.unwrap_or(1) as usize,
            _ => 1,
        };
        repeat * rotation
    }
}

impl TimelineItem {
    pub fn id(&self) -> &str {
        match self {
            Self::Block(b) => &b.id,
            Self::Group(g) => &g.id,
        }
    }

    pub fn approx_minutes(&self) -> f64 {
        match self {
            Self::Block(b) => b.approx_minutes(),
            Self::Group(g) => g.approx_minutes(),
        }
    }

    pub fn is_structural(&self) -> bool {
        matches!(self, Self::Block(b) if b.block_type.is_structural())
    }
}

impl Timeline {
    /// Sum of all non-dock items.
    pub fn planned_minutes(&self) -> f64 {
        self.items
            .iter()
            .filter(|it| !matches!(it, TimelineItem::Block(b) if b.block_type == BlockType::Dock))
            .map(|it| it.approx_minutes())
            .sum()
    }

    pub fn slack_minutes(&self) -> f64 {
        self.target_minutes as f64 - self.planned_minutes()
    }

    pub fn default_empty(target_minutes: u32) -> Self {
        Self {
            target_minutes,
            items: vec![
                TimelineItem::Block(Block {
                    id: "launch".into(),
                    block_type: BlockType::Launch,
                    duration: Duration {
                        value: 10.0,
                        unit: DurationUnit::Min,
                    },
                    note: String::new(),
                }),
                TimelineItem::Block(Block {
                    id: "dock".into(),
                    block_type: BlockType::Dock,
                    duration: Duration {
                        value: 0.0,
                        unit: DurationUnit::Min,
                    },
                    note: String::new(),
                }),
            ],
        }
    }

    /// Summarize each visible item (excluding launch/dock).
    pub fn summary_lines(&self) -> Vec<SummaryLine> {
        self.items
            .iter()
            .filter(|it| !matches!(it, TimelineItem::Block(b) if b.block_type.is_structural()))
            .map(|it| match it {
                TimelineItem::Block(b) => SummaryLine {
                    item_type: ItemDisplayType::Block(b.block_type),
                    label: b.block_type.label().to_string(),
                    duration_label: b.duration.display(),
                    rotation_label: None,
                    repeat_instruction: None,
                    children: vec![],
                    note: b.note.clone(),
                },
                TimelineItem::Group(g) => {
                    let total_reps = g.strip_repetitions();

                    // Rotation cadence — shown in group header line.
                    let rotation_label = if g.rotation.is_active() {
                        let mut parts = Vec::new();
                        if let Some(rb) = g.rotation.row_by {
                            parts.push(format!("row by {rb}"));
                        }
                        if let Some(rb) = g.rotation.rotate_by {
                            parts.push(format!("rotate by {rb}s"));
                        }
                        if let RotatePer::Every { value, unit } = &g.rotation.rotate_per {
                            let d = Duration {
                                value: *value,
                                unit: *unit,
                            };
                            parts.push(format!("every {}", d.display()));
                        }
                        if parts.is_empty() {
                            None
                        } else {
                            Some(parts.join(", "))
                        }
                    } else {
                        None
                    };

                    // Repeat pill — only actual repetition counts.
                    let repeat_instruction = if total_reps > 1 {
                        let mut parts = Vec::new();
                        if matches!(g.rotation.rotate_per, RotatePer::Group) {
                            if let Some(rb) = g.rotation.rotate_by {
                                parts.push(format!("rotate by {rb}s"));
                            }
                        }
                        parts.push(format!("{total_reps} reps"));
                        Some(parts.join(", "))
                    } else {
                        None
                    };

                    SummaryLine {
                        item_type: ItemDisplayType::Group(g.group_type),
                        label: if g.name.is_empty() {
                            g.group_type.label().to_string()
                        } else {
                            g.name.clone()
                        },
                        duration_label: format!("{:.0}'", g.approx_minutes()),
                        rotation_label,
                        repeat_instruction,
                        children: g
                            .segments
                            .iter()
                            .map(|s| SegmentSummary {
                                seg_type: s.seg_type,
                                label: summarize_segment(s),
                                note: s.note.clone(),
                            })
                            .collect(),
                        note: g.note.clone(),
                    }
                }
            })
            .collect()
    }

    /// Insert items immediately before the trailing dock.
    pub fn insert_before_dock(&mut self, new_items: Vec<TimelineItem>) {
        let dock_idx = self
            .items
            .iter()
            .position(|it| matches!(it, TimelineItem::Block(b) if b.block_type == BlockType::Dock));
        let insert_at = dock_idx.unwrap_or(self.items.len());
        for (i, item) in new_items.into_iter().enumerate() {
            self.items.insert(insert_at + i, item);
        }
    }
}

// ── Summary types ────────────────────────────────────────────────────

/// How to display an item in the summary.
pub enum ItemDisplayType {
    Block(BlockType),
    Group(GroupType),
}

pub struct SummaryLine {
    pub item_type: ItemDisplayType,
    pub label: String,
    pub duration_label: String,
    /// Rotation cadence shown in the group header (e.g. "row by 6, rotate by 2, every 20 str").
    pub rotation_label: Option<String>,
    /// Shown as a "Repeat" pill after the segment list.
    pub repeat_instruction: Option<String>,
    pub children: Vec<SegmentSummary>,
    pub note: String,
}

pub struct SegmentSummary {
    pub seg_type: SegmentType,
    pub label: String,
    pub note: String,
}

fn summarize_segment(s: &Segment) -> String {
    // Core: duration + rate + intensity
    let mut core = vec![s.duration.display()];
    if s.seg_type.is_work() {
        if let Some([lo, hi]) = s.rate {
            if lo == hi {
                core.push(format!("r{lo}"));
            } else {
                core.push(format!("r{lo}-{hi}"));
            }
        }
        if let Some(int) = s.intensity {
            core.push(format!("@{}", int.label()));
        }
    }

    // Modifiers: partial strokes, pause, blade, drills
    let mut mods = Vec::new();
    if let Some(sl) = s.partial {
        if sl != Slide::Full {
            mods.push(sl.label().to_string());
        }
    }
    if !s.pause.is_empty() {
        let labels: Vec<&str> = s.pause.iter().map(|p| p.label()).collect();
        let mut pause_str = format!("pause @ {}", labels.join(" + "));
        if let Some(every) = s.pause_every {
            if every > 1 {
                pause_str.push_str(&format!(" every {every} str"));
            }
        }
        mods.push(pause_str);
    }
    match s.blade {
        Some(Blade::Square) => mods.push("on square".to_string()),
        Some(Blade::PartialFeather) => mods.push("partial feather".to_string()),
        _ => {}
    }
    if !s.drills.is_empty() {
        let drill_labels: Vec<&str> = s.drills.iter().map(|d| d.label()).collect();
        mods.push(drill_labels.join(", "));
    }

    if mods.is_empty() {
        core.join(" ")
    } else {
        format!("{} \u{00b7} {}", core.join(" "), mods.join(" \u{00b7} "))
    }
}

// ── Built-in templates ───────────────────────────────────────────────

pub struct Template {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub group_type: GroupType,
    pub group_name: &'static str,
    /// Group repeat count. None or 1 = no repeat.
    pub repeat: Option<u8>,
    pub rotation: Rotation,
    pub segments: fn() -> Vec<Segment>,
}

fn seg(
    st: SegmentType,
    dur: f64,
    unit: DurationUnit,
    rate: Option<[u8; 2]>,
    intensity: Option<Intensity>,
    partial: Option<Slide>,
    note: &str,
) -> Segment {
    Segment {
        id: String::new(),
        seg_type: st,
        duration: Duration { value: dur, unit },
        rate,
        intensity,
        partial,
        pause: vec![],
        pause_every: None,
        blade: None,
        drills: vec![],
        note: note.to_string(),
    }
}

pub fn built_in_templates() -> Vec<Template> {
    use DurationUnit::*;
    use Intensity::*;
    use SegmentType::*;
    use Slide::*;

    vec![
        Template {
            id: "pick-drill",
            name: "Pick drill",
            description: "Arms only -> +body -> 3/4 -> 1/2 -> 1/4 -> full, by halves",
            group_type: GroupType::Warmup,
            group_name: "Pick drill",
            repeat: None,
            rotation: Rotation {
                row_by: Some(4),
                rotate_by: Some(4),
                rotate_per: RotatePer::Group,
                rotations: Some(2),
            },
            segments: || {
                vec![
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([26, 30]),
                        Some(Paddle),
                        Some(ArmsOnly),
                        "arms only, no body",
                    ),
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([24, 26]),
                        Some(Paddle),
                        Some(ArmsBody),
                        "add body swing",
                    ),
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([22, 24]),
                        Some(Paddle),
                        Some(ThreeQuarter),
                        "arms + body + short slide",
                    ),
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([20, 22]),
                        Some(Paddle),
                        Some(Half),
                        "",
                    ),
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([18, 22]),
                        Some(Paddle),
                        Some(Quarter),
                        "",
                    ),
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([18, 20]),
                        Some(Ut2),
                        Some(Full),
                        "",
                    ),
                ]
                .into_iter()
                .map(|mut s| {
                    s.blade = Some(Blade::Square);
                    s
                })
                .collect()
            },
        },
        Template {
            id: "reverse-pick",
            name: "Reverse pick drill",
            description: "1/4 -> 1/2 -> legs+body -> full, by halves",
            group_type: GroupType::Warmup,
            group_name: "Reverse pick",
            repeat: None,
            rotation: Rotation {
                row_by: Some(4),
                rotate_by: Some(4),
                rotate_per: RotatePer::Group,
                rotations: Some(2),
            },
            segments: || {
                vec![
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([24, 26]),
                        Some(Paddle),
                        Some(Quarter),
                        "legs only, no body",
                    ),
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([22, 24]),
                        Some(Paddle),
                        Some(Half),
                        "",
                    ),
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([18, 22]),
                        Some(Paddle),
                        Some(LegsBody),
                        "add body swing",
                    ),
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([18, 20]),
                        Some(Ut2),
                        Some(Full),
                        "",
                    ),
                ]
                .into_iter()
                .map(|mut s| {
                    s.blade = Some(Blade::Square);
                    s
                })
                .collect()
            },
        },
        Template {
            id: "pause-progression",
            name: "Pause progression",
            description: "Single pause at release, then two-phase",
            group_type: GroupType::Warmup,
            group_name: "Pause drill",
            repeat: None,
            rotation: Rotation::default(),
            segments: || {
                vec![
                    {
                        let mut s = seg(Work, 4.0, Min, Some([18, 20]), Some(Paddle), None, "");
                        s.pause = vec![PausePoint::Release];
                        s
                    },
                    {
                        let mut s = seg(Work, 4.0, Min, Some([18, 20]), Some(Paddle), None, "");
                        s.pause = vec![PausePoint::Release, PausePoint::ArmsAway];
                        s
                    },
                ]
            },
        },
        Template {
            id: "square-blade",
            name: "Square-blade drill",
            description: "Steady state on the square",
            group_type: GroupType::Warmup,
            group_name: "Square blade",
            repeat: None,
            rotation: Rotation::default(),
            segments: || {
                vec![{
                    let mut s = seg(Work, 6.0, Min, Some([18, 20]), Some(Paddle), None, "");
                    s.blade = Some(Blade::Square);
                    s
                }]
            },
        },
        Template {
            id: "steady-4x15",
            name: "Steady state 4x15'",
            description: "Classic UT2 set with rest",
            group_type: GroupType::Piece,
            group_name: "4x15' UT2",
            repeat: Some(4),
            rotation: Rotation::default(),
            segments: || {
                vec![
                    seg(Work, 15.0, Min, Some([20, 22]), Some(Ut2), None, ""),
                    seg(Rest, 3.0, Min, None, None, None, ""),
                ]
            },
        },
        Template {
            id: "aerobic-3x10",
            name: "Aerobic set 3x10'",
            description: "UT1 with rest",
            group_type: GroupType::Piece,
            group_name: "3x10' UT1",
            repeat: Some(3),
            rotation: Rotation::default(),
            segments: || {
                vec![
                    seg(Work, 10.0, Min, Some([22, 26]), Some(Ut1), None, ""),
                    seg(Rest, 3.0, Min, None, None, None, ""),
                ]
            },
        },
        Template {
            id: "race-3x500",
            name: "Race pieces 3x500m",
            description: "AT/TR work, 4 min rest",
            group_type: GroupType::Piece,
            group_name: "3x500m race",
            repeat: Some(3),
            rotation: Rotation::default(),
            segments: || {
                vec![
                    seg(Work, 500.0, Meters, Some([30, 34]), Some(Tr), None, ""),
                    seg(Rest, 4.0, Min, None, None, None, ""),
                ]
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_json() {
        let tl = Timeline::default_empty(90);
        let json = serde_json::to_string(&tl).unwrap();
        let back: Timeline = serde_json::from_str(&json).unwrap();
        assert_eq!(tl, back);
    }

    #[test]
    fn planned_minutes_excludes_dock() {
        let tl = Timeline::default_empty(90);
        assert!((tl.planned_minutes() - 10.0).abs() < 0.01);
    }

    #[test]
    fn slack_minutes_correct() {
        let tl = Timeline::default_empty(90);
        assert!((tl.slack_minutes() - 80.0).abs() < 0.01);
    }

    #[test]
    fn group_with_segments_round_trips() {
        let tl = Timeline {
            target_minutes: 90,
            items: vec![
                TimelineItem::Block(Block {
                    id: "launch".into(),
                    block_type: BlockType::Launch,
                    duration: Duration {
                        value: 10.0,
                        unit: DurationUnit::Min,
                    },
                    note: String::new(),
                }),
                TimelineItem::Group(Group {
                    id: "g1".into(),
                    group_type: GroupType::Warmup,
                    name: "Pick drill".into(),
                    segments: vec![
                        seg(
                            SegmentType::Work,
                            5.0,
                            DurationUnit::Min,
                            Some([18, 20]),
                            Some(Intensity::Paddle),
                            None,
                            "",
                        ),
                        seg(
                            SegmentType::Rest,
                            2.0,
                            DurationUnit::Min,
                            None,
                            None,
                            None,
                            "",
                        ),
                    ],
                    repeat: None,
                    rotation: Rotation::default(),
                    note: String::new(),
                }),
                TimelineItem::Block(Block {
                    id: "dock".into(),
                    block_type: BlockType::Dock,
                    duration: Duration {
                        value: 0.0,
                        unit: DurationUnit::Min,
                    },
                    note: String::new(),
                }),
            ],
        };
        let json = serde_json::to_string(&tl).unwrap();
        let back: Timeline = serde_json::from_str(&json).unwrap();
        assert_eq!(tl, back);
    }

    #[test]
    fn summarize_segment_basic() {
        let s = seg(
            SegmentType::Work,
            15.0,
            DurationUnit::Min,
            Some([20, 22]),
            Some(Intensity::Ut2),
            None,
            "",
        );
        assert_eq!(summarize_segment(&s), "15' r20-22 @UT2");
    }
}
