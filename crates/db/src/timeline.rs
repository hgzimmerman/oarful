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
    pub modifiers: Vec<Modifier>,
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
    #[serde(default)]
    pub modifiers: Vec<Modifier>,
    #[serde(default)]
    pub note: String,
}

/// Segment types inside a group.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
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

// ── Drill / modifier enums (unchanged) ──────────────────────────────

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum Intensity {
    Paddle,
    #[serde(rename = "ut2")]
    #[strum(serialize = "ut2")]
    Ut2,
    #[serde(rename = "ut1")]
    #[strum(serialize = "ut1")]
    Ut1,
    #[serde(rename = "at")]
    #[strum(serialize = "at")]
    At,
    #[serde(rename = "tr")]
    #[strum(serialize = "tr")]
    Tr,
    #[serde(rename = "an")]
    #[strum(serialize = "an")]
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

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
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

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
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

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
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

// ── Unified modifier ─────────────────────────────────────────────────

/// A modifier attached to a Group or Segment.
///
/// Group-level modifiers inherit to all child segments.  A segment can
/// override an inherited modifier by carrying its own `Modifier` of the
/// same kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Modifier {
    #[serde(rename = "blade")]
    Blade { value: Blade },
    #[serde(rename = "partial")]
    Partial { value: Slide },
    #[serde(rename = "pause_at")]
    PauseAt {
        points: Vec<PausePoint>,
        #[serde(default)]
        every: Option<u32>,
    },
    #[serde(rename = "drills")]
    Drills { values: Vec<HandDrill> },
    #[serde(rename = "emphasis")]
    Emphasis { text: String },
    #[serde(rename = "repeating_emphasis")]
    RepeatingEmphasis {
        /// How often (e.g. every 2 minutes).
        every: u32,
        /// Unit for the interval.
        every_unit: DurationUnit,
        /// How many strokes per burst (e.g. 10).
        count: u32,
        /// Short name shown on the plan (e.g. "power 10").
        label: String,
    },
    /// Row by N rowers at a time (e.g. by 2s, by 4s). Segment-only.
    #[serde(rename = "row_by")]
    RowBy { value: u8 },
}

impl Modifier {
    /// Whether this modifier cascades from group to segments.
    /// Emphasis and RepeatingEmphasis are group-level notes that don't inherit.
    pub fn cascades(&self) -> bool {
        !matches!(
            self,
            Self::Emphasis { .. } | Self::RepeatingEmphasis { .. } | Self::RowBy { .. }
        )
    }

    /// String key used to match modifiers across group↔segment for inheritance.
    pub fn kind_id(&self) -> &'static str {
        match self {
            Self::Blade { .. } => "blade",
            Self::Partial { .. } => "partial",
            Self::PauseAt { .. } => "pause_at",
            Self::Drills { .. } => "drills",
            Self::Emphasis { .. } => "emphasis",
            Self::RepeatingEmphasis { .. } => "repeating_emphasis",
            Self::RowBy { .. } => "row_by",
        }
    }

    /// Human-readable label.
    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::Blade { .. } => "Blade",
            Self::Partial { .. } => "Partial strokes",
            Self::PauseAt { .. } => "Pause at",
            Self::Drills { .. } => "Drills",
            Self::Emphasis { .. } => "Notes",
            Self::RepeatingEmphasis { .. } => "Repeating",
            Self::RowBy { .. } => "Row by",
        }
    }

    /// Default value for a freshly-added modifier.
    pub fn default_for_kind(kind_id: &str) -> Option<Self> {
        match kind_id {
            "blade" => Some(Self::Blade {
                value: Blade::Feather,
            }),
            "partial" => Some(Self::Partial { value: Slide::Full }),
            "pause_at" => Some(Self::PauseAt {
                points: vec![],
                every: None,
            }),
            "drills" => Some(Self::Drills { values: vec![] }),
            "emphasis" => Some(Self::Emphasis {
                text: String::new(),
            }),
            "repeating_emphasis" => Some(Self::RepeatingEmphasis {
                every: 2,
                every_unit: DurationUnit::Min,
                count: 10,
                label: "power".to_string(),
            }),
            "row_by" => Some(Self::RowBy { value: 4 }),
            _ => None,
        }
    }

    /// Short summary of this modifier's value for display in badges/summaries.
    pub fn summary_label(&self) -> String {
        match self {
            Self::Blade { value } => value.label().to_string(),
            Self::Partial { value } => value.label().to_string(),
            Self::PauseAt { points, every } => {
                if points.is_empty() {
                    return String::new();
                }
                let labels: Vec<&str> = points.iter().map(|p| p.label()).collect();
                let mut s = format!("pause @ {}", labels.join(" + "));
                if let Some(e) = every {
                    if *e > 1 {
                        s.push_str(&format!(" every {e} str"));
                    }
                }
                s
            }
            Self::Drills { values } => values
                .iter()
                .map(|d| d.label())
                .collect::<Vec<_>>()
                .join(", "),
            Self::Emphasis { text } => text.clone(),
            Self::RepeatingEmphasis {
                every,
                every_unit,
                count,
                label,
            } => {
                let unit = every_unit.label();
                let name = if label.is_empty() { "emphasis" } else { label };
                format!("{name} {count} every {every}{unit}")
            }
            Self::RowBy { value } => format!("by {value}s"),
        }
    }
}

/// Catalogue entry for the modifier picker UI.
pub struct ModifierCatalogueEntry {
    pub kind_id: &'static str,
    pub name: &'static str,
    pub group: &'static str,
    pub description: &'static str,
    pub value_shape: &'static str,
}

/// All known modifier kinds, in picker display order.
pub fn modifier_catalogue() -> Vec<ModifierCatalogueEntry> {
    vec![
        ModifierCatalogueEntry {
            kind_id: "blade",
            name: "Blade",
            group: "Stroke shape",
            description: "feather · partial feather · on square",
            value_shape: "picks one",
        },
        ModifierCatalogueEntry {
            kind_id: "partial",
            name: "Partial strokes",
            group: "Stroke shape",
            description: "full · arms only · arms + body · \u{00bc} · \u{00bd} · \u{00be} slide",
            value_shape: "picks one",
        },
        ModifierCatalogueEntry {
            kind_id: "pause_at",
            name: "Pause at",
            group: "Stroke shape",
            description: "release · arms away · bodies over · \u{00bd} slide · catch",
            value_shape: "multi",
        },
        ModifierCatalogueEntry {
            kind_id: "drills",
            name: "Drills",
            group: "Skill focus",
            description: "feet out, inside arm, cut the cake, gunnel taps\u{2026}",
            value_shape: "multi",
        },
        ModifierCatalogueEntry {
            kind_id: "emphasis",
            name: "Notes",
            group: "Skill focus",
            description: "a coaching cue, e.g. \u{201c}connection at the catch\u{201d}",
            value_shape: "free text",
        },
        ModifierCatalogueEntry {
            kind_id: "repeating_emphasis",
            name: "Repeating emphasis",
            group: "Pacing",
            description: "power 10s, focus 5s \u{2014} every N min or strokes",
            value_shape: "compound",
        },
        ModifierCatalogueEntry {
            kind_id: "row_by",
            name: "Row by",
            group: "Pacing",
            description: "row by 2s, 4s, 6s, or 8s",
            value_shape: "picks one",
        },
    ]
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
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
                    modifier_labels: vec![],
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

                    let modifier_labels: Vec<String> = g
                        .modifiers
                        .iter()
                        .map(|m| format!("{}: {}", m.kind_label(), m.summary_label()))
                        .filter(|s| !s.ends_with(": "))
                        .collect();

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
                        modifier_labels,
                        children: g
                            .segments
                            .iter()
                            .map(|s| SegmentSummary {
                                seg_type: s.seg_type,
                                parts: summarize_segment_with_modifiers(s, &g.modifiers),
                                note: s.note.clone(),
                            })
                            .collect(),
                        note: g.note.clone(),
                    }
                }
            })
            .collect()
    }

    /// Insert items after the item with the given ID, or before the dock if not found.
    pub fn insert_after_item(&mut self, after_id: &str, new_items: Vec<TimelineItem>) {
        // Find the top-level item matching after_id (or containing a segment with that ID).
        let after_idx = self.items.iter().position(|it| {
            it.id() == after_id
                || matches!(it, TimelineItem::Group(g) if g.segments.iter().any(|s| s.id == after_id))
        });
        let insert_at = match after_idx {
            Some(idx) => {
                // Insert after the matched item, but never past the dock.
                let dock_idx = self
                    .items
                    .iter()
                    .position(|it| {
                        matches!(it, TimelineItem::Block(b) if b.block_type == BlockType::Dock)
                    })
                    .unwrap_or(self.items.len());
                (idx + 1).min(dock_idx)
            }
            None => {
                // Fallback: before dock.
                self.items
                    .iter()
                    .position(|it| {
                        matches!(it, TimelineItem::Block(b) if b.block_type == BlockType::Dock)
                    })
                    .unwrap_or(self.items.len())
            }
        };
        for (i, item) in new_items.into_iter().enumerate() {
            self.items.insert(insert_at + i, item);
        }
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

// ── Versioned envelope ───────────────────────────────────────────────
//
// Wraps Timeline in a version tag for forward-compatible serialization.
// Deserialization accepts both the versioned envelope and the bare
// legacy format (target_minutes + items at the top level).

/// Versioned envelope for timeline JSON. Always serialize as V1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "version")]
enum TimelineVersioned {
    #[serde(rename = "1")]
    V1 { timeline: Timeline },
}

impl Timeline {
    /// Serialize to JSON with a version envelope.
    pub fn to_json(&self) -> String {
        let envelope = TimelineVersioned::V1 {
            timeline: self.clone(),
        };
        serde_json::to_string(&envelope).unwrap_or_else(|_| "{}".to_string())
    }

    /// Deserialize from JSON, accepting both versioned and legacy formats.
    pub fn from_json(s: &str) -> Option<Self> {
        // Try versioned envelope first.
        if let Ok(TimelineVersioned::V1 { timeline }) = serde_json::from_str(s) {
            return Some(timeline);
        }
        // Fall back to legacy bare format.
        serde_json::from_str(s).ok()
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
    /// Group-level modifier summaries.
    pub modifier_labels: Vec<String>,
    pub children: Vec<SegmentSummary>,
    pub note: String,
}

pub struct SegmentSummary {
    pub seg_type: SegmentType,
    /// Structured parts: core label followed by modifier spans.
    pub parts: Vec<SegmentSummaryPart>,
    pub note: String,
}

/// One piece of a segment summary line — either plain text or modifier-tagged.
#[derive(Debug, PartialEq)]
pub struct SegmentSummaryPart {
    pub text: String,
    /// `None` for core text (duration/rate/intensity), `Some(kind_id)` for modifier spans.
    pub modifier_kind: Option<&'static str>,
}

/// Summarize a segment with no group modifiers.
#[allow(dead_code)]
pub fn summarize_segment(s: &Segment) -> Vec<SegmentSummaryPart> {
    summarize_segment_with_modifiers(s, &[])
}

/// Summarize a segment, merging its own modifiers with any inherited
/// group-level modifiers. Group modifiers are shown unless the segment
/// has an override of the same kind.
fn summarize_segment_with_modifiers(
    s: &Segment,
    group_modifiers: &[Modifier],
) -> Vec<SegmentSummaryPart> {
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

    let mut parts = vec![SegmentSummaryPart {
        text: core.join(" "),
        modifier_kind: None,
    }];

    // Collect effective modifiers: group-level (cascading, unless overridden) + segment-level
    for gm in group_modifiers {
        if !gm.cascades() {
            continue;
        }
        if s.modifiers.iter().any(|m| m.kind_id() == gm.kind_id()) {
            continue;
        }
        let label = gm.summary_label();
        if !label.is_empty() {
            parts.push(SegmentSummaryPart {
                text: label,
                modifier_kind: Some(gm.kind_id()),
            });
        }
    }
    for m in &s.modifiers {
        let label = m.summary_label();
        if !label.is_empty() {
            parts.push(SegmentSummaryPart {
                text: label,
                modifier_kind: Some(m.kind_id()),
            });
        }
    }

    parts
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
    /// Group-level modifiers that inherit to all segments.
    pub modifiers: Vec<Modifier>,
    pub segments: fn() -> Vec<Segment>,
}

fn seg(
    st: SegmentType,
    dur: f64,
    unit: DurationUnit,
    rate: Option<[u8; 2]>,
    intensity: Option<Intensity>,
    modifiers: Vec<Modifier>,
    note: &str,
) -> Segment {
    Segment {
        id: String::new(),
        seg_type: st,
        duration: Duration { value: dur, unit },
        rate,
        intensity,
        modifiers,
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
            modifiers: vec![Modifier::Blade {
                value: Blade::Square,
            }],
            segments: || {
                vec![
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([26, 30]),
                        Some(Paddle),
                        vec![Modifier::Partial { value: ArmsOnly }],
                        "arms only, no body",
                    ),
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([24, 26]),
                        Some(Paddle),
                        vec![Modifier::Partial { value: ArmsBody }],
                        "add body swing",
                    ),
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([22, 24]),
                        Some(Paddle),
                        vec![Modifier::Partial {
                            value: ThreeQuarter,
                        }],
                        "arms + body + short slide",
                    ),
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([20, 22]),
                        Some(Paddle),
                        vec![Modifier::Partial { value: Half }],
                        "",
                    ),
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([18, 22]),
                        Some(Paddle),
                        vec![Modifier::Partial { value: Quarter }],
                        "",
                    ),
                    seg(Work, 10.0, Strokes, Some([18, 20]), Some(Ut2), vec![], ""),
                ]
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
            modifiers: vec![Modifier::Blade {
                value: Blade::Square,
            }],
            segments: || {
                vec![
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([24, 26]),
                        Some(Paddle),
                        vec![Modifier::Partial { value: Quarter }],
                        "legs only, no body",
                    ),
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([22, 24]),
                        Some(Paddle),
                        vec![Modifier::Partial { value: Half }],
                        "",
                    ),
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([18, 22]),
                        Some(Paddle),
                        vec![Modifier::Partial { value: LegsBody }],
                        "add body swing",
                    ),
                    seg(Work, 10.0, Strokes, Some([18, 20]), Some(Ut2), vec![], ""),
                ]
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
            modifiers: vec![],
            segments: || {
                vec![
                    seg(
                        Work,
                        4.0,
                        Min,
                        Some([18, 20]),
                        Some(Paddle),
                        vec![Modifier::PauseAt {
                            points: vec![PausePoint::Release],
                            every: None,
                        }],
                        "",
                    ),
                    seg(
                        Work,
                        4.0,
                        Min,
                        Some([18, 20]),
                        Some(Paddle),
                        vec![Modifier::PauseAt {
                            points: vec![PausePoint::Release, PausePoint::ArmsAway],
                            every: None,
                        }],
                        "",
                    ),
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
            modifiers: vec![Modifier::Blade {
                value: Blade::Square,
            }],
            segments: || {
                vec![seg(
                    Work,
                    6.0,
                    Min,
                    Some([18, 20]),
                    Some(Paddle),
                    vec![],
                    "",
                )]
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
            modifiers: vec![],
            segments: || {
                vec![
                    seg(Work, 15.0, Min, Some([20, 22]), Some(Ut2), vec![], ""),
                    seg(Rest, 3.0, Min, None, None, vec![], ""),
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
            modifiers: vec![],
            segments: || {
                vec![
                    seg(Work, 10.0, Min, Some([22, 26]), Some(Ut1), vec![], ""),
                    seg(Rest, 3.0, Min, None, None, vec![], ""),
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
            modifiers: vec![],
            segments: || {
                vec![
                    seg(Work, 500.0, Meters, Some([30, 34]), Some(Tr), vec![], ""),
                    seg(Rest, 4.0, Min, None, None, vec![], ""),
                ]
            },
        },
        Template {
            id: "build-drill",
            name: "Build drill",
            description: "By 2s → 4s → 6s → 8s, 10 strokes each, 4 reps",
            group_type: GroupType::Warmup,
            group_name: "Build drill",
            repeat: Some(4),
            rotation: Rotation::default(),
            modifiers: vec![],
            segments: || {
                vec![
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([20, 22]),
                        Some(Paddle),
                        vec![Modifier::RowBy { value: 2 }],
                        "",
                    ),
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([20, 22]),
                        Some(Paddle),
                        vec![Modifier::RowBy { value: 4 }],
                        "",
                    ),
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([20, 22]),
                        Some(Paddle),
                        vec![Modifier::RowBy { value: 6 }],
                        "",
                    ),
                    seg(
                        Work,
                        10.0,
                        Strokes,
                        Some([20, 22]),
                        Some(Paddle),
                        vec![Modifier::RowBy { value: 8 }],
                        "",
                    ),
                    seg(
                        Rest,
                        0.5,
                        Min,
                        None,
                        None,
                        vec![],
                        "check it down, rotate starting pair",
                    ),
                ]
            },
        },
        Template {
            id: "start-seq-half",
            name: "Start sequence (1/2 lead)",
            description: "Progressive start: 1/2 1/2 3/4 lengthen full + power 5",
            group_type: GroupType::Warmup,
            group_name: "Start sequence",
            repeat: None,
            rotation: Rotation::default(),
            modifiers: vec![],
            segments: || {
                let r = Some([36, 38]);
                let p = |s| vec![Modifier::Partial { value: s }];
                vec![
                    // Attempt 1: just stroke 1
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(Half), "Pry!"),
                    seg(Rest, 0.25, Min, None, None, vec![], ""),
                    // Attempt 2: strokes 1-2
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(Half), "Pry!"),
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(Half), ""),
                    seg(Rest, 0.25, Min, None, None, vec![], ""),
                    // Attempt 3: strokes 1-3
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(Half), "Pry!"),
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(Half), ""),
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(ThreeQuarter), ""),
                    seg(Rest, 0.25, Min, None, None, vec![], ""),
                    // Attempt 4: strokes 1-4
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(Half), "Pry!"),
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(Half), ""),
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(ThreeQuarter), ""),
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(ThreeQuarter), "lengthen"),
                    seg(Rest, 0.25, Min, None, None, vec![], ""),
                    // Attempt 5: full start + power 5
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(Half), "Pry!"),
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(Half), ""),
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(ThreeQuarter), ""),
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(ThreeQuarter), "lengthen"),
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(Full), "full"),
                    seg(Work, 5.0, Strokes, r, Some(Tr), vec![], "power 5"),
                ]
            },
        },
        Template {
            id: "start-seq-three-quarter",
            name: "Start sequence (3/4 lead)",
            description: "Progressive start: 3/4 1/2 1/2 lengthen full + power 5",
            group_type: GroupType::Warmup,
            group_name: "Start sequence",
            repeat: None,
            rotation: Rotation::default(),
            modifiers: vec![],
            segments: || {
                let r = Some([36, 38]);
                let p = |s| vec![Modifier::Partial { value: s }];
                vec![
                    // Attempt 1: just stroke 1
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(ThreeQuarter), "Pry!"),
                    seg(Rest, 0.25, Min, None, None, vec![], ""),
                    // Attempt 2: strokes 1-2
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(ThreeQuarter), "Pry!"),
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(Half), ""),
                    seg(Rest, 0.25, Min, None, None, vec![], ""),
                    // Attempt 3: strokes 1-3
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(ThreeQuarter), "Pry!"),
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(Half), ""),
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(Half), ""),
                    seg(Rest, 0.25, Min, None, None, vec![], ""),
                    // Attempt 4: strokes 1-4
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(ThreeQuarter), "Pry!"),
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(Half), ""),
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(Half), ""),
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(ThreeQuarter), "lengthen"),
                    seg(Rest, 0.25, Min, None, None, vec![], ""),
                    // Attempt 5: full start + power 5
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(ThreeQuarter), "Pry!"),
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(Half), ""),
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(Half), ""),
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(ThreeQuarter), "lengthen"),
                    seg(Work, 1.0, Strokes, r, Some(Tr), p(Full), "full"),
                    seg(Work, 5.0, Strokes, r, Some(Tr), vec![], "power 5"),
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
                            vec![],
                            "",
                        ),
                        seg(
                            SegmentType::Rest,
                            2.0,
                            DurationUnit::Min,
                            None,
                            None,
                            vec![],
                            "",
                        ),
                    ],
                    repeat: None,
                    rotation: Rotation::default(),
                    modifiers: vec![],
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
            vec![],
            "",
        );
        let parts = summarize_segment(&s);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].text, "15' r20-22 @UT2");
        assert_eq!(parts[0].modifier_kind, None);
    }

    // ── strum Display ↔ serde Deserialize round-trip ──

    /// Verify that every variant's `Display` output (from strum) round-trips
    /// through serde `Deserialize`. This catches drift between the strum
    /// `serialize_all` and the serde `rename_all` attributes.
    fn assert_display_serde_round_trip<T>(variants: &[T])
    where
        T: std::fmt::Display + std::fmt::Debug + PartialEq + serde::de::DeserializeOwned,
    {
        for v in variants {
            let s = v.to_string();
            let back: T = serde_json::from_value(serde_json::Value::String(s.clone()))
                .unwrap_or_else(|e| panic!("{s:?} (from {v:?}) failed to deserialize: {e}"));
            assert_eq!(
                &back, v,
                "round-trip mismatch: Display produced {s:?}, deserialized to {back:?}, expected {v:?}"
            );
        }
    }

    #[test]
    fn segment_type_display_matches_serde() {
        assert_display_serde_round_trip(&[SegmentType::Work, SegmentType::Rest, SegmentType::Turn]);
    }

    #[test]
    fn intensity_display_matches_serde() {
        assert_display_serde_round_trip(Intensity::ALL);
    }

    #[test]
    fn slide_display_matches_serde() {
        assert_display_serde_round_trip(Slide::ALL);
    }

    #[test]
    fn pause_point_display_matches_serde() {
        assert_display_serde_round_trip(PausePoint::ALL);
    }

    #[test]
    fn hand_drill_display_matches_serde() {
        assert_display_serde_round_trip(HandDrill::ALL);
    }

    #[test]
    fn versioned_round_trip() {
        let tl = Timeline::default_empty(90);
        let json = tl.to_json();
        // Should contain version tag.
        assert!(json.contains(r#""version":"1""#));
        let back = Timeline::from_json(&json).unwrap();
        assert_eq!(tl, back);
    }

    #[test]
    fn legacy_json_deserializes() {
        // Legacy format: bare Timeline without version envelope.
        let tl = Timeline::default_empty(60);
        let legacy_json = serde_json::to_string(&tl).unwrap();
        assert!(!legacy_json.contains("version"));
        let back = Timeline::from_json(&legacy_json).unwrap();
        assert_eq!(tl, back);
    }

    #[test]
    fn duration_unit_display_matches_serde() {
        assert_display_serde_round_trip(&[
            DurationUnit::Min,
            DurationUnit::Meters,
            DurationUnit::Strokes,
        ]);
    }
}
