//! Timeline visual strip — the horizontal bar showing all items proportionally.

use lineup_db::timeline::{BlockType, Timeline, TimelineItem};
use maud::{html, Markup};

use super::css::{strip_bg_block, strip_bg_group, strip_bg_seg};
use super::tooltips::{block_tooltip, format_duration_short, group_tooltip, segment_tooltip};

pub(super) fn timeline_strip(
    tl: &Timeline,
    base_url: &str,
    tl_json: &str,
    selected_id: Option<&str>,
) -> Markup {
    let slack = tl.slack_minutes();
    let total_min: f64 = tl
        .items
        .iter()
        .map(|it| match it {
            TimelineItem::Block(b) if b.block_type == BlockType::Dock => slack.max(0.0),
            _ => it.approx_minutes(),
        })
        .sum::<f64>()
        .max(1.0);

    html! {
        div id="tl-strip" class="flex gap-px mb-3 rounded overflow-hidden" style="height: 45px; background: var(--paper-2)" {
            @for item in &tl.items {
                @let id = item.id();
                @let is_selected = selected_id == Some(id) || match item {
                    TimelineItem::Group(g) => selected_id.is_some_and(|sid| g.segments.iter().any(|s| s.id == sid)),
                    _ => false,
                };
                @let border = if is_selected { "outline: 1.5px solid var(--ink-3); outline-offset: -1.5px; z-index: 1" } else { "" };

                @match item {
                    TimelineItem::Block(b) => {
                        @let is_dock = b.block_type == BlockType::Dock;
                        @let minutes = if is_dock { slack.max(0.0) } else { b.approx_minutes() };
                        @let pct = (minutes / total_min * 100.0).max(2.0);
                        @let bg = if is_dock {
                            "background: repeating-linear-gradient(135deg, color-mix(in oklch, var(--cox) 8%, var(--paper)) 0px, color-mix(in oklch, var(--cox) 8%, var(--paper)) 4px, color-mix(in oklch, var(--cox) 14%, var(--paper)) 4px, color-mix(in oklch, var(--cox) 14%, var(--paper)) 8px)"
                        } else { strip_bg_block(b.block_type) };
                        @let tooltip = block_tooltip(b, is_dock, slack);
                        @let is_structural = b.block_type.is_structural();
                        form class="inline" style={"flex: " (format!("{:.2}", pct)) "; min-width: 0; " (bg) "; " (border)}
                             title=(tooltip) hx-post={(base_url) "/patch-block"} hx-target="#timeline-section" hx-swap="innerHTML"
                             draggable={@if !is_structural { "true" } @else { "false" }}
                             data-tl-id=(id)
                             data-drag-id=[(!is_structural).then_some(id)]
                             data-drop-id=[(!is_structural).then_some(id)]
                             data-drag-zone="strip" {
                            input type="hidden" name="timeline" value=(tl_json);
                            input type="hidden" name="patch_id" value=(id);
                            input type="hidden" name="selected" value=(id);
                            button type="submit" class="w-full h-full flex flex-col items-center cursor-pointer overflow-hidden px-1 pt-1"
                                   style="background: none; border: none; font: inherit" {
                                span class="font-mono-stat text-[8px] uppercase tracking-wider truncate w-full text-center" style="opacity: 0.7; pointer-events: none" { (b.block_type.label()) }
                                @if minutes > 0.0 {
                                    span class="font-mono-stat text-[9px] truncate w-full text-center" style="pointer-events: none" {
                                        @if is_dock {
                                            @if slack > 0.0 { "+" (format!("{:.0}", slack)) "m" }
                                            @else { "0m" }
                                        } @else { (format_duration_short(minutes)) }
                                    }
                                }
                            }
                        }
                    }
                    TimelineItem::Group(g) => {
                        @let minutes = g.approx_minutes().max(0.01);
                        @let pct = (minutes / total_min * 100.0).max(2.0);
                        @let bg = strip_bg_group(g.group_type);
                        div style={"flex: " (format!("{:.2}", pct)) "; min-width: 0; " (bg) "; " (border)
                                    "; display: flex; flex-direction: column; border-radius: 2px; overflow: hidden"}
                            draggable="true"
                            data-tl-id=(id)
                            data-drag-id=(id)
                            data-drop-id=(id)
                            data-drag-zone="strip" {
                            @if !g.name.is_empty() {
                                form class="block" style="line-height: 0"
                                     hx-post={(base_url) "/group-patch"} hx-target="#timeline-section" hx-swap="innerHTML" {
                                    input type="hidden" name="timeline" value=(tl_json);
                                    input type="hidden" name="group_id" value=(id);
                                    input type="hidden" name="selected" value=(g.segments.first().map(|s| s.id.as_str()).unwrap_or(id));
                                    button type="submit" class="w-full cursor-pointer truncate"
                                           style="font-size: 8px; font-family: ui-monospace, 'Cascadia Code', 'SF Mono', Menlo, monospace; letter-spacing: 0.08em; text-transform: uppercase; color: var(--ink-3); line-height: 1.4; background: none; border: none; padding: 4px 2px 0"
                                           title=(group_tooltip(g)) {
                                        (g.name)
                                    }
                                }
                            }
                            @let reps = g.strip_repetitions();
                            @let seg_count = g.segments.len();
                            @let total_bars = seg_count * reps;
                            div class="flex gap-px flex-1" style="min-height: 0" {
                                @for bar_idx in 0..total_bars {
                                    @let s = &g.segments[bar_idx % seg_count];
                                    @let sm = s.approx_minutes();
                                    @let spct = sm.max(0.1);
                                    @let seg_selected = selected_id == Some(s.id.as_str());
                                    @let seg_border = if seg_selected { "outline: 1.5px solid var(--ink-3); outline-offset: -1.5px; z-index: 1" } else { "" };
                                    @let rotation_gap = if bar_idx > 0 && bar_idx % seg_count == 0 { "margin-left: 2px" } else { "" };
                                    @let seg_tip = segment_tooltip(s);
                                    form class="inline" style={"flex: " (format!("{:.1}", spct)) "; min-width: 0; " (strip_bg_seg(s.seg_type, g.group_type)) "; border-radius: 1px; " (seg_border) "; " (rotation_gap)}
                                         title=(seg_tip) hx-post={(base_url) "/group-patch"} hx-target="#timeline-section" hx-swap="innerHTML" {
                                        input type="hidden" name="timeline" value=(tl_json);
                                        input type="hidden" name="group_id" value=(id);
                                        input type="hidden" name="selected" value=(s.id);
                                        button type="submit" class="w-full h-full cursor-pointer"
                                               style="background: none; border: none; font: inherit; padding: 0" {}
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
