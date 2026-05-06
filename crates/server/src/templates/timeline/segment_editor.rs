//! Segment detail editor (intensity, rate, modifiers, drills).

use lineup_db::timeline::{Blade, Group, HandDrill, Intensity, PausePoint, Segment, Slide};
use maud::{html, Markup};

use super::css::chip_style;
use super::helpers::{duration_field, field_label};

pub(super) fn segment_editor(
    seg: &Segment,
    group: &Group,
    base_url: &str,
    tl_json: &str,
    animate: bool,
) -> Markup {
    let is_work = seg.seg_type.is_work();
    html! {
        div class={"mt-3 pt-3" @if animate { " tl-animate-in" }} style="border-top: 1px dashed var(--rule-2)" {
            form hx-post={(base_url) "/patch-segment"} hx-target="#timeline-section" hx-swap="outerHTML" hx-trigger="change" {
                input type="hidden" name="timeline" value=(tl_json);
                input type="hidden" name="group_id" value=(group.id);
                input type="hidden" name="segment_id" value=(seg.id);
                input type="hidden" name="selected" value=(seg.id);

                div class="space-y-3" {
                    // Duration + Intensity
                    div class="flex flex-wrap gap-4 items-start" {
                        (duration_field(seg.duration.value, seg.duration.unit))
                        @if is_work {
                            div {
                                (field_label("Intensity"))
                                div class="flex flex-wrap gap-1" {
                                    @for int in Intensity::ALL {
                                        @let is_active = seg.intensity == Some(*int);
                                        label class="cursor-pointer" {
                                            input type="radio" name="intensity" value=(int) checked[is_active] class="hidden";
                                            span class={"font-mono-stat text-[10px] px-1.5 py-0.5 rounded border cursor-pointer " @if is_active { "font-bold" }}
                                                 style=(chip_style(is_active))
                                                 title=(int.full_name()) { (int.label()) }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Stroke rate
                    @if is_work {
                        div {
                            (field_label("Stroke rate"))
                            @let rate = seg.rate.unwrap_or([20, 20]);
                            @let is_range = rate[0] != rate[1];
                            div class="flex items-center gap-2 flex-wrap" {
                                div class="flex items-center gap-1" {
                                    span class="font-mono-stat text-xs" style="color: var(--muted)" { "r" }
                                    input type="range" name="rate_low" min="10" max="50" value=(rate[0]) class="range-warm" style="width: 100px";
                                    span class="font-mono-stat text-xs font-medium w-5 text-center" style="color: var(--ink)" { (rate[0]) }
                                }
                                @if is_range {
                                    div class="flex items-center gap-1" {
                                        span class="font-mono-stat text-[9px]" style="color: var(--muted)" { "to" }
                                        input type="range" name="rate_high" min="10" max="50" value=(rate[1]) class="range-warm" style="width: 100px";
                                        span class="font-mono-stat text-xs font-medium w-5 text-center" style="color: var(--ink)" { (rate[1]) }
                                    }
                                } @else {
                                    input type="hidden" name="rate_high" value=(rate[0]);
                                }
                                label class="flex items-center gap-1 cursor-pointer ml-1" title="Toggle rate range" {
                                    input type="checkbox" name="_range_toggle" checked[is_range] class="hidden";
                                    span class={"font-mono-stat text-[9px] px-1 py-0.5 rounded border cursor-pointer " @if is_range { "font-bold" }}
                                         style=(chip_style(is_range)) { "range" }
                                }
                            }
                        }
                    }

                    // Modifiers
                    @if is_work {
                        div class="flex flex-wrap gap-4 items-start" {
                            // Partial strokes
                            div {
                                (field_label("Partial strokes"))
                                select name="partial" class="input-warm text-xs py-0.5" {
                                    @for s in Slide::ALL {
                                        option value=(s) selected[seg.partial == Some(*s) || (*s == Slide::Full && seg.partial.is_none())] { (s.label()) }
                                    }
                                }
                            }
                            // Blade
                            div {
                                (field_label("Blade"))
                                div class="flex gap-1" {
                                    @for (val, lbl) in &[("feather", "feather"), ("partial-feather", "partial feather"), ("square", "on square")] {
                                        @let is_active = match *val {
                                            "square" => seg.blade == Some(Blade::Square),
                                            "partial-feather" => seg.blade == Some(Blade::PartialFeather),
                                            _ => seg.blade.is_none() || seg.blade == Some(Blade::Feather),
                                        };
                                        label class="cursor-pointer" {
                                            input type="radio" name="blade" value=(val) checked[is_active] class="hidden";
                                            span class={"font-mono-stat text-[9px] px-1 py-0.5 rounded border cursor-pointer " @if is_active { "font-bold" }}
                                                 style=(chip_style(is_active)) { (lbl) }
                                        }
                                    }
                                }
                            }
                            // Pause
                            @let pause_csv = seg.pause.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",");
                            div {
                                (field_label("Pause at"))
                                input type="hidden" name="pause_points" value=(pause_csv) data-multiselect="pause";
                                div class="flex flex-wrap gap-1" {
                                    @for pp in PausePoint::ALL {
                                        @let is_active = seg.pause.contains(pp);
                                        label class="cursor-pointer" {
                                            input type="checkbox" value=(pp) checked[is_active] class="hidden"
                                                  onchange="event.stopPropagation();var h=this.form.querySelector('[data-multiselect=pause]');var vs=[];this.form.querySelectorAll('input[type=checkbox][onchange*=pause]:checked').forEach(function(c){vs.push(c.value)});h.value=vs.join(',');h.dispatchEvent(new Event('change',{bubbles:true}))";
                                            span class={"font-mono-stat text-[9px] px-1 py-0.5 rounded border cursor-pointer " @if is_active { "font-bold" }}
                                                 style=(chip_style(is_active)) { (pp.label()) }
                                        }
                                    }
                                }
                                @if !seg.pause.is_empty() {
                                    div class="flex items-center gap-1 mt-1" {
                                        label class="font-mono-stat text-[9px]" style="color: var(--muted)" { "every" }
                                        input type="number" name="pause_every" min="1" max="20"
                                              value=(seg.pause_every.unwrap_or(1))
                                              class="input-warm text-xs w-14 py-0.5 text-center";
                                        span class="font-mono-stat text-[9px]" style="color: var(--muted)" { "strokes" }
                                    }
                                }
                            }
                            // Drills
                            @let drills_csv = seg.drills.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(",");
                            div {
                                (field_label("Drills"))
                                input type="hidden" name="drills" value=(drills_csv) data-multiselect="drills";
                                div class="flex flex-wrap gap-1" {
                                    @for hd in HandDrill::ALL {
                                        @let is_active = seg.drills.contains(hd);
                                        label class="cursor-pointer" {
                                            input type="checkbox" value=(hd) checked[is_active] class="hidden"
                                                  onchange="event.stopPropagation();var h=this.form.querySelector('[data-multiselect=drills]');var vs=[];this.form.querySelectorAll('input[type=checkbox][onchange*=drills]:checked').forEach(function(c){vs.push(c.value)});h.value=vs.join(',');h.dispatchEvent(new Event('change',{bubbles:true}))";
                                            span class={"font-mono-stat text-[9px] px-1 py-0.5 rounded border cursor-pointer " @if is_active { "font-bold" }}
                                                 style=(chip_style(is_active)) { (hd.label()) }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Emphasis / note
                    div {
                        (field_label(if is_work { "Emphasis" } else { "Notes" }))
                        textarea name="note" rows="1" placeholder={@if is_work { "e.g. connection at the catch" } @else { "e.g. paddle between pieces" }}
                                 class="input-warm text-sm w-full resize-y" { (seg.note) }
                    }
                }
            }
        }
    }
}
