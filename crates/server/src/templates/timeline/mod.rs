//! Timeline editor + summary templates.

mod block_editor;
mod css;
mod group_editor;
mod helpers;
mod segment_editor;
mod strip;
mod tooltips;

use lineup_db::timeline::{
    self, Block, BlockType, Group, GroupType, ItemDisplayType, SegmentSummaryPart, Timeline,
    TimelineItem,
};
use maud::{html, Markup};

use super::history::block_type_css;
use css::{group_type_css, seg_type_css};

/// Editor visibility state, driven by `?plan_editor=` query param.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
pub enum PlanEditorState {
    /// No param — show collapsed summary.
    #[default]
    #[serde(rename = "")]
    Closed,
    /// `?plan_editor=open` — show editor only.
    #[serde(rename = "open")]
    Open,
    /// `?plan_editor=open_preview` — show editor + preview panel.
    #[serde(rename = "open_preview")]
    OpenPreview,
}

impl PlanEditorState {
    pub fn is_open(self) -> bool {
        !matches!(self, Self::Closed)
    }

    pub fn has_preview(self) -> bool {
        matches!(self, Self::OpenPreview)
    }

    /// Value for the hidden form field.
    pub fn as_param(self) -> &'static str {
        match self {
            Self::Closed => "",
            Self::Open => "open",
            Self::OpenPreview => "open_preview",
        }
    }
}

// ── Summary (collapsed view) ─────────────────────────────────────────

pub(crate) fn summary(tl: &Timeline, base_url: &str) -> Markup {
    section_wrapper(summary_content(tl, base_url, None, None))
}

pub(crate) fn practice_summary(
    tl: &Timeline,
    base_url: &str,
    import_url: &str,
    skip_url: Option<&str>,
) -> Markup {
    section_wrapper(summary_content(tl, base_url, Some(import_url), skip_url))
}

pub(crate) fn summary_content(
    tl: &Timeline,
    base_url: &str,
    import_url: Option<&str>,
    skip_url: Option<&str>,
) -> Markup {
    let lines = tl.summary_lines();
    let planned = tl.planned_minutes();
    let slack = tl.slack_minutes();

    html! {
        div {
            div class="flex items-baseline justify-between gap-2 mb-1" {
                span class="font-mono-stat text-[9.5px] tracking-[0.16em] uppercase font-semibold" style="color: var(--accent)" {
                    "Practice plan"
                }
                div class="flex items-center gap-3" {
                    span class="font-mono-stat text-[10px]" style="color: var(--muted)" {
                        (format!("{:.0}", planned)) " of " (tl.target_minutes) " min"
                    }
                    @if slack > 0.0 {
                        span class="font-mono-stat text-[10px]" style="color: var(--good)" {
                            "+" (format!("{:.0}", slack)) " slack"
                        }
                    } @else if slack < -1.0 {
                        span class="font-mono-stat text-[10px]" style="color: var(--bad)" {
                            (format!("{:.0}", slack)) " over"
                        }
                    }
                    button class="font-mono-stat text-[10.5px] hover:underline cursor-pointer px-1"
                           style="color: var(--muted); background: none; border: none"
                           hx-get={(base_url) "/edit?plan_editor=open"}
                           hx-target="#timeline-section"
                           hx-swap="innerHTML"
                           "hx-push-url"={(base_url.replace("/timeline", "/detail")) "?plan_editor=open"} {
                        "edit plan"
                    }
                    @if let Some(url) = import_url {
                        button class="font-mono-stat text-[10.5px] hover:underline cursor-pointer px-1"
                               style="color: var(--muted); background: none; border: none"
                               hx-get=(url)
                               hx-target="body"
                               hx-swap="beforeend" {
                            "use template"
                        }
                    }
                    @if let Some(url) = skip_url {
                        button class="font-mono-stat text-[10.5px] hover:underline cursor-pointer px-1"
                               style="color: var(--muted); background: none; border: none"
                               hx-post=(url)
                               hx-target="#content" {
                            "skip plan"
                        }
                    }
                }
            }
            @if lines.is_empty() {
                p class="text-sm italic m-0" style="color: var(--muted)" { "No plan yet." }
            } @else {
                ol class="list-none m-0 p-0 space-y-1" {
                    @for line in &lines {
                        li {
                            @match &line.item_type {
                                ItemDisplayType::Group(gt) => {
                                    div class="flex items-center gap-2" {
                                        span class="font-mono-stat text-[9px] px-1 py-px rounded border"
                                             style=(group_type_css(*gt)) {
                                            (gt.label())
                                        }
                                        span class="font-serif-heading font-medium text-sm" style="color: var(--ink)" {
                                            (line.label)
                                        }
                                        span class="font-mono-stat text-[9px]" style="color: var(--muted)" {
                                            (line.duration_label)
                                        }
                                        @if let Some(ref rot) = line.rotation_label {
                                            span class="font-mono-stat text-[9px] italic" style="color: var(--muted)" {
                                                " · " (rot)
                                            }
                                        }
                                    }
                                    @if !line.modifier_labels.is_empty() {
                                        div class="pl-5 mt-0.5" {
                                            @for ml in &line.modifier_labels {
                                                span class="font-mono-stat text-[9px] italic mr-2" style="color: var(--accent)" { (ml) }
                                            }
                                        }
                                    }
                                    ul class="list-none m-0 pl-5 mt-0.5 space-y-0.5" {
                                        @for seg in &line.children {
                                            li class="flex items-center gap-1.5 text-xs" {
                                                span class="font-mono-stat text-[9px] px-1 py-px rounded border"
                                                     style=(seg_type_css(seg.seg_type)) {
                                                    (seg.seg_type.label())
                                                }
                                                (segment_parts_markup(&seg.parts))
                                                @if !seg.note.is_empty() {
                                                    span class="italic" style="color: var(--muted)" { "— " (seg.note) }
                                                }
                                            }
                                        }
                                        @if let Some(ref instr) = line.repeat_instruction {
                                            li class="flex items-center gap-1.5 text-xs mt-0.5" {
                                                span class="font-mono-stat text-[9px] px-1 py-px rounded border"
                                                     style="color: var(--bad); background: color-mix(in oklch, var(--bad) 10%, var(--paper)); border-color: color-mix(in oklch, var(--bad) 25%, var(--rule))" {
                                                    "Repeat"
                                                }
                                                span class="font-mono-stat" style="color: var(--ink-2)" { (instr) }
                                            }
                                        }
                                    }
                                }
                                ItemDisplayType::Block(bt) => {
                                    div class="flex items-center gap-1.5 text-xs" {
                                        span class="font-mono-stat text-[9px] px-1 py-px rounded border" style=(block_type_css(*bt)) {
                                            (bt.label())
                                        }
                                        span class="font-mono-stat" style="color: var(--ink-2)" { (line.duration_label) }
                                        @if !line.note.is_empty() {
                                            span class="italic" style="color: var(--muted)" { "— " (line.note) }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Read-only preview of plan contents (no header, no actions).
pub(crate) fn preview(tl: &Timeline) -> Markup {
    let lines = tl.summary_lines();
    let planned = tl.planned_minutes();
    html! {
        div class="space-y-1" {
            span class="font-mono-stat text-[10px]" style="color: var(--muted)" {
                (format!("{:.0}", planned)) " min planned"
            }
            @if lines.is_empty() {
                p class="text-sm italic" style="color: var(--muted)" { "Empty plan." }
            } @else {
                ol class="list-none m-0 p-0 space-y-1" {
                    @for line in &lines {
                        li {
                            @match &line.item_type {
                                ItemDisplayType::Group(gt) => {
                                    div class="flex items-center gap-2" {
                                        span class="font-mono-stat text-[9px] px-1 py-px rounded border"
                                             style=(group_type_css(*gt)) {
                                            (gt.label())
                                        }
                                        span class="font-serif-heading font-medium text-sm" style="color: var(--ink)" {
                                            (line.label)
                                        }
                                        span class="font-mono-stat text-[9px]" style="color: var(--muted)" {
                                            (line.duration_label)
                                        }
                                        @if let Some(ref rot) = line.rotation_label {
                                            span class="font-mono-stat text-[9px] italic" style="color: var(--muted)" {
                                                " · " (rot)
                                            }
                                        }
                                    }
                                    @if !line.modifier_labels.is_empty() {
                                        div class="pl-5 mt-0.5" {
                                            @for ml in &line.modifier_labels {
                                                span class="font-mono-stat text-[9px] italic mr-2" style="color: var(--accent)" { (ml) }
                                            }
                                        }
                                    }
                                    ul class="list-none m-0 pl-5 mt-0.5 space-y-0.5" {
                                        @for seg in &line.children {
                                            li class="flex items-center gap-1.5 text-xs" {
                                                span class="font-mono-stat text-[9px] px-1 py-px rounded border"
                                                     style=(seg_type_css(seg.seg_type)) {
                                                    (seg.seg_type.label())
                                                }
                                                (segment_parts_markup(&seg.parts))
                                                @if !seg.note.is_empty() {
                                                    span class="italic" style="color: var(--muted)" { "— " (seg.note) }
                                                }
                                            }
                                        }
                                        @if let Some(ref instr) = line.repeat_instruction {
                                            li class="flex items-center gap-1.5 text-xs mt-0.5" {
                                                span class="font-mono-stat text-[9px] px-1 py-px rounded border"
                                                     style="color: var(--bad); background: color-mix(in oklch, var(--bad) 10%, var(--paper)); border-color: color-mix(in oklch, var(--bad) 25%, var(--rule))" {
                                                    "Repeat"
                                                }
                                                span class="font-mono-stat" style="color: var(--ink-2)" { (instr) }
                                            }
                                        }
                                    }
                                }
                                ItemDisplayType::Block(bt) => {
                                    div class="flex items-center gap-1.5 text-xs" {
                                        span class="font-mono-stat text-[9px] px-1 py-px rounded border" style=(block_type_css(*bt)) {
                                            (bt.label())
                                        }
                                        span class="font-mono-stat" style="color: var(--ink-2)" { (line.duration_label) }
                                        @if !line.note.is_empty() {
                                            span class="italic" style="color: var(--muted)" { "— " (line.note) }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Editor (expanded view) ───────────────────────────────────────────

pub(crate) fn editor_content(
    tl: &Timeline,
    base_url: &str,
    selected_id: Option<&str>,
    editor_state: PlanEditorState,
) -> Markup {
    let tl_json = serde_json::to_string(tl).unwrap_or_else(|_| "{}".to_string());
    let pe = editor_state.as_param();
    let planned = tl.planned_minutes();
    let slack = tl.slack_minutes();
    let slack_state = if slack > 5.0 {
        "ok"
    } else if slack >= -1.0 {
        "tight"
    } else {
        "over"
    };

    enum Sel<'a> {
        Block(&'a Block),
        Group(&'a Group),
        None,
    }
    let selected = match selected_id {
        Some(sid) => {
            let mut found = Sel::None;
            for item in &tl.items {
                match item {
                    TimelineItem::Block(b) if b.id == sid => {
                        found = Sel::Block(b);
                        break;
                    }
                    TimelineItem::Group(g) if g.id == sid => {
                        found = Sel::Group(g);
                        break;
                    }
                    TimelineItem::Group(g) if g.segments.iter().any(|s| s.id == sid) => {
                        found = Sel::Group(g);
                        break;
                    }
                    _ => {}
                }
            }
            found
        }
        None => Sel::None,
    };

    html! {
        // Header
        div class="flex items-center justify-between mb-3" {
            div class="flex items-baseline gap-2" {
                span class="font-mono-stat text-[9.5px] tracking-[0.16em] uppercase font-semibold" style="color: var(--accent)" { "Practice plan" }
                span class="font-mono-stat text-[9px]" style="color: var(--muted)" { " · click a block to edit" }
            }
            div class="flex items-center gap-3" {
                // Duration meter
                div class="flex items-center gap-1" {
                    span class="font-mono-stat text-sm font-medium" style="color: var(--ink)" { (format!("{:.0}", planned)) }
                    span class="font-mono-stat text-[10px]" style="color: var(--muted)" { "/" }
                    form class="inline" hx-post={(base_url) "/target"} hx-target="#timeline-section" hx-swap="innerHTML" {
                        input type="hidden" name="timeline" value=(tl_json);
                        input type="hidden" name="selected" value=(selected_id.unwrap_or(""));
                        input type="hidden" name="plan_editor" value=(pe);
                        input type="number" name="new_target" min="20" max="240" value=(tl.target_minutes)
                              class="font-mono-stat text-sm font-medium w-10 text-center border-b"
                              style="color: var(--ink); background: transparent; border-color: var(--rule); border-width: 0 0 1px 0; padding: 0; outline: none"
                              onchange="this.form.requestSubmit()";
                        span class="font-mono-stat text-[10px]" style="color: var(--muted)" { " min" }
                    }
                }
                @match slack_state {
                    "ok" => { span class="font-mono-stat text-[10px]" style="color: var(--good)" { "+" (format!("{:.0}", slack)) " slack" } }
                    "tight" => { span class="font-mono-stat text-[10px]" style="color: var(--warn)" { "tight" } }
                    _ => { span class="font-mono-stat text-[10px]" style="color: var(--bad)" { (format!("{:.0}", slack)) " over" } }
                }
                // Preview toggle
                @let detail_url = base_url.replace("/timeline", "/detail");
                @let toggle_state = if editor_state.has_preview() { "open" } else { "open_preview" };
                @let toggle_label = if editor_state.has_preview() { "Hide preview" } else { "Preview" };
                form class="inline" hx-post={(base_url) "/target"} hx-target="#timeline-section" hx-swap="innerHTML"
                     "hx-push-url"={(&detail_url) "?plan_editor=" (toggle_state)} {
                    input type="hidden" name="timeline" value=(tl_json);
                    input type="hidden" name="selected" value=(selected_id.unwrap_or(""));
                    input type="hidden" name="plan_editor" value=(toggle_state);
                    input type="hidden" name="new_target" value=(tl.target_minutes);
                    button type="submit" class="font-mono-stat text-[10px] px-2 py-1 rounded border cursor-pointer hover:opacity-80"
                           style=(if editor_state.has_preview() {
                               "color: var(--accent); border-color: color-mix(in oklch, var(--accent) 30%, var(--rule)); background: color-mix(in oklch, var(--accent) 8%, var(--paper))"
                           } else {
                               "color: var(--ink-2); border-color: var(--rule); background: var(--paper)"
                           }) {
                        (toggle_label)
                    }
                }
                form class="inline" hx-post={(base_url) "/save"} hx-target="#timeline-section" hx-swap="innerHTML"
                     "hx-push-url"=(&detail_url) {
                    input type="hidden" name="timeline" value=(tl_json);
                    input type="hidden" name="plan_editor" value=(pe);
                    button type="submit" class="btn-warm-ink text-xs py-1.5 px-3" { "Save" }
                }
                button class="btn-warm-ghost text-xs py-1.5 px-3"
                       hx-post={(base_url) "/close"} hx-target="#timeline-section" hx-swap="innerHTML"
                       "hx-push-url"=(&detail_url) { "Cancel" }
            }
        }

        // Palette
        div class="flex flex-wrap gap-1.5 items-center mb-3 pb-3" style="border-bottom: 1px solid var(--rule-2)" {
            span class="font-mono-stat text-[9px] tracking-wider uppercase self-center mr-1" style="color: var(--muted)" { "Add" }
            @for (add_type, label, css) in &[
                ("warmup", "Warmup", group_type_css(GroupType::Warmup)),
                ("piece", "Piece", group_type_css(GroupType::Piece)),
            ] {
                form class="inline" hx-post={(base_url) "/add"} hx-target="#timeline-section" hx-swap="innerHTML" {
                    input type="hidden" name="timeline" value=(tl_json);
                    input type="hidden" name="selected" value=(selected_id.unwrap_or(""));
                    input type="hidden" name="plan_editor" value=(pe);
                    input type="hidden" name="add_type" value=(add_type);
                    button type="submit" class="font-mono-stat text-[10px] px-2 py-1 rounded border cursor-pointer hover:opacity-80" style=(css) { (label) }
                }
            }
            @for bt in BlockType::USER_ADDABLE {
                form class="inline" hx-post={(base_url) "/add"} hx-target="#timeline-section" hx-swap="innerHTML" {
                    input type="hidden" name="timeline" value=(tl_json);
                    input type="hidden" name="selected" value=(selected_id.unwrap_or(""));
                    input type="hidden" name="plan_editor" value=(pe);
                    input type="hidden" name="add_type" value=(bt.label().to_lowercase());
                    button type="submit" class="font-mono-stat text-[10px] px-2 py-1 rounded border cursor-pointer hover:opacity-80" style=(block_type_css(*bt)) { (bt.label()) }
                }
            }
            // "From template" dropdown
            @let templates = timeline::built_in_templates();
            @let tmpl_js_arr = templates.iter().map(|t|
                format!("{{n:'{}',d:'{}',id:'{}'}}", t.name.replace('\'', "\\'"), t.description.replace('\'', "\\'"), t.id)
            ).collect::<Vec<_>>().join(",");
            div class="relative inline-block"
                 x-data=(maud::PreEscaped(format!("{{ open: false, search: '', sel: -1, tmpls: [{}], get visible() {{ if (!this.search) return this.tmpls; var s = this.search.toLowerCase(); return this.tmpls.filter(t => t.n.toLowerCase().includes(s) || t.d.toLowerCase().includes(s)); }}, get matchCount() {{ return this.visible.length; }}, matches(t) {{ if (!this.search) return true; var s = this.search.toLowerCase(); return t.n.toLowerCase().includes(s) || t.d.toLowerCase().includes(s); }} }}", tmpl_js_arr))) {
                button type="button" class="font-mono-stat text-[10px] px-2 py-1 rounded border cursor-pointer hover:opacity-80"
                       style="color: var(--ink-2); border-color: var(--rule); background: var(--paper)"
                       "@click"="open = !open; sel = -1; if (open) $nextTick(() => $refs.tmplSearch.focus())" {
                    "From template"
                }
                div x-show="open" x-cloak=""
                    "@click.outside"="open = false"
                    "@keydown.escape.window"="open = false"
                    class="absolute left-0 top-full mt-1 z-50 rounded-lg overflow-hidden"
                    style="background: var(--paper); border: 1px solid var(--rule); min-width: 320px; box-shadow: var(--shadow-card)" {
                    // Search with result count
                    div class="flex items-center gap-2 p-2" {
                        input type="text" x-model="search" x-ref="tmplSearch" placeholder="Search templates…"
                              class="input-warm text-sm flex-1 py-1.5 px-2"
                              "@input"="sel = -1"
                              "@keydown.arrow-down.prevent"="sel = Math.min(sel + 1, matchCount - 1)"
                              "@keydown.arrow-up.prevent"="sel = Math.max(sel - 1, -1)"
                              "@keydown.enter.prevent"="if (sel >= 0 && sel < matchCount) { var id = visible[sel].id; var btn = $root.querySelector('[data-tmpl-id=\"' + id + '\"]'); if (btn) btn.click(); }";
                        span class="font-mono-stat text-[9px] flex-shrink-0" style="color: var(--muted)"
                             x-text="matchCount + ' result' + (matchCount === 1 ? '' : 's')" {}
                    }
                    div class="max-h-96 overflow-y-auto" x-ref="tmplList" style="border-top: 1px solid var(--rule-2)" {
                        @for gt in &[GroupType::Warmup, GroupType::Piece] {
                            @let group_tmpls: Vec<_> = templates.iter().filter(|t| t.group_type == *gt).collect();
                            @if !group_tmpls.is_empty() {
                                @let group_match_expr = group_tmpls.iter().map(|t|
                                    format!("'{}'.toLowerCase().includes(search.toLowerCase()) || '{}'.toLowerCase().includes(search.toLowerCase())",
                                        t.name.replace('\'', "\\'"), t.description.replace('\'', "\\'"))
                                ).collect::<Vec<_>>().join(" || ");
                                div x-show={ "!search || " (group_match_expr) }  {
                                    // Group header with count
                                    div class="font-mono-stat text-[8px] tracking-[0.12em] uppercase px-3 pt-3 pb-1" style="color: var(--muted)" {
                                        (gt.label()) " · " (group_tmpls.len())
                                    }
                                    @for tmpl in &group_tmpls {
                                        @let match_expr = format!("matches({{ n: '{}', d: '{}' }})",
                                            tmpl.name.replace('\'', "\\'"), tmpl.description.replace('\'', "\\'"));
                                        form class="block" hx-post={(base_url) "/template"} hx-target="#timeline-section" hx-swap="innerHTML" {
                                            input type="hidden" name="timeline" value=(tl_json);
                                            input type="hidden" name="selected" value=(selected_id.unwrap_or(""));
                                            input type="hidden" name="plan_editor" value=(pe);
                                            input type="hidden" name="template_id" value=(tmpl.id);
                                            button type="submit"
                                                   x-show=(match_expr)
                                                   data-tmpl-id=(tmpl.id)
                                                   class="tmpl-item w-full text-left px-3 py-2 cursor-pointer flex items-start gap-2.5"
                                                   ":class"=(maud::PreEscaped(format!("{{ 'tmpl-sel': sel >= 0 && visible[sel] && visible[sel].id === '{}' }}", tmpl.id)))
                                                   style="border: none; color: var(--ink)"
                                                   "@mouseenter"="sel = -1" {
                                                // Group type badge
                                                span class="font-mono-stat text-[10px] font-bold w-5 h-5 rounded flex items-center justify-center mt-0.5 flex-shrink-0"
                                                     style=(group_type_css(tmpl.group_type)) {
                                                    @match tmpl.group_type {
                                                        GroupType::Warmup => { "W" }
                                                        GroupType::Piece => { "P" }
                                                    }
                                                }
                                                div class="flex-1 min-w-0" {
                                                    div class="text-sm font-semibold" { (tmpl.name) }
                                                    div class="text-xs mt-0.5 truncate" style="color: var(--muted)" { (tmpl.description) }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Footer with keyboard hints
                    div class="flex items-center gap-2 px-3 py-1.5 font-mono-stat text-[9px]"
                         style="border-top: 1px solid var(--rule-2); color: var(--muted)" {
                        span class="px-1 py-px rounded" style="background: var(--paper-2); border: 1px solid var(--rule)" { "↑" }
                        span class="px-1 py-px rounded" style="background: var(--paper-2); border: 1px solid var(--rule)" { "↓" }
                        span { "navigate" }
                        span { "·" }
                        span class="px-1 py-px rounded" style="background: var(--paper-2); border: 1px solid var(--rule)" { "↵" }
                        span { "add" }
                    }
                }
            }
        }

        // Hidden reorder forms
        form id="tl-reorder-form" class="hidden"
             hx-post={(base_url) "/reorder"}
             hx-target="#timeline-section"
             hx-swap="innerHTML" {
            input type="hidden" name="timeline" value=(tl_json);
            input type="hidden" name="plan_editor" value=(pe);
            input type="hidden" name="drag_id" value="";
            input type="hidden" name="drop_before_id" value="";
        }
        form id="tl-seg-reorder-form" class="hidden"
             hx-post={(base_url) "/group-reorder"}
             hx-target="#timeline-section"
             hx-swap="innerHTML" {
            input type="hidden" name="timeline" value=(tl_json);
            input type="hidden" name="plan_editor" value=(pe);
            input type="hidden" name="group_id" value=(selected_id.unwrap_or(""));
            input type="hidden" name="drag_id" value="";
            input type="hidden" name="drop_before_id" value="";
        }

        // Timeline strip
        (strip::timeline_strip(tl, base_url, &tl_json, selected_id, pe))

        // Editor for selected item
        @match &selected {
            Sel::Block(block) => { (block_editor::bare_block_editor(block, base_url, &tl_json, pe)) }
            Sel::Group(group) => { (group_editor::group_editor(group, base_url, &tl_json, selected_id, pe)) }
            Sel::None => {}
        }

        // Summary preview panel (sibling to editor)
        @if editor_state.has_preview() {
            div class="mt-4 pt-3 pl-4 py-3" style="border-left: 3px solid var(--accent); border-top: 1px solid var(--rule-2)" {
                div class="flex items-baseline gap-2 mb-2" {
                    span class="font-mono-stat text-[9px] tracking-[0.12em] uppercase font-semibold" style="color: var(--accent)" { "Summary preview" }
                }
                (preview(tl))
            }
        }

        // Drag-and-drop reorder JS + strip FLIP animation
        (maud::PreEscaped(DRAG_REORDER_JS))
        (maud::PreEscaped(STRIP_FLIP_JS))
    }
}

/// Render segment summary parts: core text + dotted-underline modifier spans.
fn segment_parts_markup(parts: &[SegmentSummaryPart]) -> Markup {
    html! {
        span class="font-mono-stat" style="color: var(--ink-2)" {
            @for (i, part) in parts.iter().enumerate() {
                @if let Some(kind) = part.modifier_kind {
                    span class="mod-dot" { " \u{00b7} " }
                    span class="mod-span" data-kind=(kind)
                         title=(format!("{} modifier", css::modifier_kind_label(kind))) {
                        (part.text)
                    }
                } @else {
                    @if i > 0 { " " }
                    (part.text)
                }
            }
        }
    }
}

pub(crate) fn section_wrapper(content: Markup) -> Markup {
    html! {
        div id="timeline-section" class="rounded-lg pl-4 py-1 mb-4" style="border-left: 3px solid var(--accent)" {
            (content)
        }
        (maud::PreEscaped(HEIGHT_TRANSITION_JS))
    }
}

const HEIGHT_TRANSITION_JS: &str = r#"<script>
(function(){
  var prevHeight = null;
  document.addEventListener('htmx:beforeSwap', function(e) {
    if (e.detail.target && e.detail.target.id === 'timeline-section') {
      prevHeight = e.detail.target.offsetHeight;
    }
  });
  document.addEventListener('htmx:afterSwap', function(e) {
    if (prevHeight === null) return;
    var el = document.getElementById('timeline-section');
    if (!el) { prevHeight = null; return; }
    var newHeight = el.offsetHeight;
    var old = prevHeight;
    prevHeight = null;
    if (Math.abs(newHeight - old) < 2) return;
    el.style.height = old + 'px';
    el.style.overflow = 'hidden';
    requestAnimationFrame(function() {
      el.style.transition = 'height 300ms ease-out';
      el.style.height = newHeight + 'px';
      el.addEventListener('transitionend', function handler() {
        el.style.height = '';
        el.style.overflow = '';
        el.style.transition = '';
        el.removeEventListener('transitionend', handler);
      });
    });
  });
})();
</script>"#;

const DRAG_REORDER_JS: &str = r#"<script>
(function(){
  var dragId = null;
  var dragZone = null;
  var dropSide = null; // 'before' or 'after'
  var allClasses = ['tl-drop-before','tl-drop-after','tl-drop-above','tl-drop-below'];

  function clearDrop() {
    allClasses.forEach(function(c){
      document.querySelectorAll('.'+c).forEach(function(d){ d.classList.remove(c); });
    });
    dropSide = null;
  }

  function findTarget(el) {
    while (el) {
      if (el.dataset && el.dataset.dropId) return el;
      el = el.parentElement;
    }
    return null;
  }

  // Which half of the element is the cursor in?
  function getSide(el, e, zone) {
    var rect = el.getBoundingClientRect();
    if (zone === 'seglist') {
      return (e.clientY < rect.top + rect.height / 2) ? 'before' : 'after';
    }
    return (e.clientX < rect.left + rect.width / 2) ? 'before' : 'after';
  }

  function sideClass(side, zone) {
    if (zone === 'seglist') return side === 'before' ? 'tl-drop-above' : 'tl-drop-below';
    return side === 'before' ? 'tl-drop-before' : 'tl-drop-after';
  }

  document.querySelectorAll('[data-drag-id]').forEach(function(el){
    el.addEventListener('dragstart', function(e){
      dragId = el.dataset.dragId;
      dragZone = el.dataset.dragZone || 'strip';
      setTimeout(function(){
        el.classList.add('tl-dragging');
        el.style.pointerEvents = 'none';
      }, 0);
      e.dataTransfer.effectAllowed = 'move';
      e.dataTransfer.setData('text/plain', dragId);
    });
    el.addEventListener('dragend', function(){
      dragId = null;
      el.classList.remove('tl-dragging');
      el.style.pointerEvents = '';
      clearDrop();
    });
  });

  var section = document.getElementById('timeline-section');
  if (!section) return;

  section.addEventListener('dragover', function(e){
    if (!dragId) return;
    var target = findTarget(e.target);
    if (!target || target.dataset.dropId === dragId) { clearDrop(); return; }
    if (target.dataset.dragZone && target.dataset.dragZone !== dragZone) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    var side = getSide(target, e, dragZone);
    clearDrop();
    target.classList.add(sideClass(side, dragZone));
    dropSide = side;
  });

  section.addEventListener('drop', function(e){
    if (!dragId) return;
    var target = findTarget(e.target);
    if (!target || target.dataset.dropId === dragId) { clearDrop(); return; }
    e.preventDefault();
    var side = getSide(target, e, dragZone);
    var targetId = target.dataset.dropId;

    // For 'after', we need the next sibling's ID as drop_before_id.
    // If there's no next sibling, use a sentinel to append at end.
    if (side === 'after') {
      var siblings = target.parentElement.querySelectorAll('[data-drop-id]');
      var arr = Array.from(siblings).filter(function(s){ return s.dataset.dragZone === dragZone; });
      var idx = arr.indexOf(target);
      if (idx >= 0 && idx + 1 < arr.length) {
        targetId = arr[idx + 1].dataset.dropId;
      } else {
        targetId = '__end__';
      }
    }

    var form = (dragZone === 'seglist')
      ? document.getElementById('tl-seg-reorder-form')
      : document.getElementById('tl-reorder-form');
    if (!form) { clearDrop(); return; }
    form.querySelector('[name=drag_id]').value = dragId;
    form.querySelector('[name=drop_before_id]').value = targetId;
    // For segment reorder, resolve group_id from the list container.
    if (dragZone === 'seglist') {
      var listEl = target.closest('[data-seg-group-id]');
      if (listEl) form.querySelector('[name=group_id]').value = listEl.dataset.segGroupId;
    }
    clearDrop();
    htmx.trigger(form, 'submit');
  });
})();
</script>"#;

const STRIP_FLIP_JS: &str = r#"<script>
(function(){
  var oldRects = null;

  document.addEventListener('htmx:beforeSwap', function(e) {
    if (!e.detail.target || e.detail.target.id !== 'timeline-section') return;
    var strip = document.getElementById('tl-strip');
    if (!strip) { oldRects = null; return; }
    oldRects = {};
    strip.querySelectorAll('[data-tl-id]').forEach(function(el) {
      var r = el.getBoundingClientRect();
      oldRects[el.dataset.tlId] = { left: r.left, top: r.top, width: r.width, height: r.height, node: el.cloneNode(true) };
    });
  });

  document.addEventListener('htmx:afterSwap', function(e) {
    if (!oldRects) return;
    var saved = oldRects;
    oldRects = null;

    var strip = document.getElementById('tl-strip');
    if (!strip) return;

    var newIds = {};
    strip.querySelectorAll('[data-tl-id]').forEach(function(el) {
      newIds[el.dataset.tlId] = true;
      var id = el.dataset.tlId;
      var nr = el.getBoundingClientRect();

      if (saved[id]) {
        var or = saved[id];
        var dx = or.left - nr.left;
        var dw = or.width / (nr.width || 1);
        if (Math.abs(dx) < 1 && Math.abs(dw - 1) < 0.02) return;
        el.style.transformOrigin = 'left center';
        el.style.transform = 'translateX(' + dx + 'px) scaleX(' + dw + ')';
        void el.offsetWidth;
        el.style.transition = 'transform 280ms ease-out';
        el.style.transform = '';
        el.addEventListener('transitionend', function h(ev) {
          if (ev.propertyName !== 'transform') return;
          el.style.transition = '';
          el.style.transformOrigin = '';
          el.removeEventListener('transitionend', h);
        });
      } else {
        el.style.transformOrigin = 'left center';
        el.style.transform = 'scaleX(0)';
        el.style.opacity = '0';
        void el.offsetWidth;
        el.style.transition = 'transform 280ms ease-out, opacity 200ms ease-out';
        el.style.transform = 'scaleX(1)';
        el.style.opacity = '1';
        el.addEventListener('transitionend', function h(ev) {
          if (ev.propertyName !== 'transform') return;
          el.style.transition = '';
          el.style.transform = '';
          el.style.transformOrigin = '';
          el.style.opacity = '';
          el.removeEventListener('transitionend', h);
        });
      }
    });

    // Deleted items: overlay a clone and collapse it leftward
    Object.keys(saved).forEach(function(id) {
      if (newIds[id]) return;
      var or = saved[id];
      var ghost = or.node;
      ghost.style.position = 'fixed';
      ghost.style.left = or.left + 'px';
      ghost.style.top = or.top + 'px';
      ghost.style.width = or.width + 'px';
      ghost.style.height = or.height + 'px';
      ghost.style.zIndex = '10';
      ghost.style.pointerEvents = 'none';
      ghost.style.transformOrigin = 'left center';
      ghost.style.margin = '0';
      document.body.appendChild(ghost);
      void ghost.offsetWidth;
      ghost.style.transition = 'transform 250ms ease-in, opacity 200ms ease-in';
      ghost.style.transform = 'scaleX(0)';
      ghost.style.opacity = '0';
      ghost.addEventListener('transitionend', function h(ev) {
        if (ev.propertyName !== 'transform') return;
        ghost.remove();
        ghost.removeEventListener('transitionend', h);
      });
    });
  });
})();
</script>"#;
