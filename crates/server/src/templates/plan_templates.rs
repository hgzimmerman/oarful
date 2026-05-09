//! Plan template management page templates.

use lineup_db::plan_template::{Category, PlanTemplate};
use lineup_db::practice::PracticeId;
use lineup_db::timeline::Timeline;
use maud::{html, Markup};

use super::layout;

// ── List content (tab body) ──────────────────────────────────────────

pub(crate) fn list_content(templates: &[PlanTemplate]) -> Markup {
    html! {
        header class="border-b px-4 sm:px-8 py-3 sm:py-4" style="border-color: var(--rule); background: var(--paper)" {
            div class="flex items-center justify-between" {
                div {
                    h1 class="font-serif-heading text-2xl font-medium tracking-tight" style="color: var(--ink)" { "Plan templates" }
                    p class="font-mono-stat text-xs tracking-wide mt-1" style="color: var(--muted)" {
                        (templates.len()) " template" @if templates.len() != 1 { "s" }
                    }
                }
                button class="btn-warm-ink text-sm py-2 px-4"
                       hx-post="/admin/plan-templates"
                       hx-target="#admin-tab-content"
                       hx-push-url="/admin/plan-templates" {
                    "New template"
                }
            }
        }
        div class="px-4 sm:px-8 py-6" {
            @if templates.is_empty() {
                (layout::empty_state("No templates yet. Create one to save a reusable practice plan."))
            } @else {
                div class="space-y-2 max-w-3xl mx-auto" {
                    @for tmpl in templates {
                        @let tl = tmpl.timeline();
                        @let planned = tl.as_ref().map(|t| t.planned_minutes()).unwrap_or(0.0);
                        a href=(format!("/admin/plan-templates/{}", tmpl.id))
                          hx-get=(format!("/admin/plan-templates/{}", tmpl.id))
                          hx-target="#admin-tab-content"
                          hx-push-url="true"
                          class="block rounded-lg px-4 py-3 hover:opacity-80 transition-opacity cursor-pointer"
                          style="background: var(--paper); box-shadow: var(--shadow-soft)" {
                            div class="flex items-center justify-between" {
                                div {
                                    span class="font-serif-heading font-medium text-sm" style="color: var(--ink)" {
                                        (tmpl.name)
                                    }
                                    @if !tmpl.description.is_empty() {
                                        span class="text-xs ml-2" style="color: var(--muted)" {
                                            (tmpl.description)
                                        }
                                    }
                                }
                                span class="font-mono-stat text-[10px]" style="color: var(--muted)" {
                                    (format!("{:.0}", planned)) " min"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Detail content (tab body) ────────────────────────────────────────

pub(crate) fn detail_content(
    tmpl: &PlanTemplate,
    tmpl_cats: &[Category],
    all_cats: &[Category],
) -> Markup {
    let tl = tmpl
        .timeline()
        .unwrap_or_else(|| Timeline::default_empty(90));
    let base_url = format!("/admin/plan-templates/{}/timeline", tmpl.id);

    html! {
        header class="border-b px-4 sm:px-8 py-3 sm:py-4" style="border-color: var(--rule); background: var(--paper)" {
            div class="flex items-center gap-3" {
                button class="text-sm hover:underline cursor-pointer"
                       style="color: var(--muted); background: none; border: none"
                       hx-get="/admin/plan-templates"
                       hx-target="#admin-tab-content"
                       hx-push-url="true" {
                    "← Templates"
                }
                h1 id="template-name" class="font-serif-heading text-xl font-medium tracking-tight" style="color: var(--ink)" { (tmpl.name) }
            }
        }
        div class="px-4 sm:px-8 py-6 max-w-4xl mx-auto space-y-6" {
            // Meta section
            div id="template-meta" {
                (meta_section(tmpl, tmpl_cats, all_cats))
            }

            // Timeline editor
            div class="mt-6" {
                (super::timeline::summary(&tl, &base_url))
            }

            // Actions
            div class="flex items-center gap-3 pt-4" style="border-top: 1px solid var(--rule)" {
                button class="btn-warm-ghost text-xs py-1.5 px-3"
                       hx-post=(format!("/admin/plan-templates/{}/duplicate", tmpl.id))
                       hx-target="#admin-tab-content" {
                    "Duplicate"
                }
                button class="text-xs py-1.5 px-3 rounded border cursor-pointer hover:opacity-80"
                       style="color: var(--bad); border-color: color-mix(in oklch, var(--bad) 30%, var(--rule)); background: transparent"
                       hx-post=(format!("/admin/plan-templates/{}/delete", tmpl.id))
                       hx-target="#admin-tab-content"
                       hx-confirm="Delete this template?" {
                    "Delete"
                }
            }
        }
    }
}

// ── Meta section (editable name/description/categories) ──────────────

pub(crate) fn meta_section(
    tmpl: &PlanTemplate,
    tmpl_cats: &[Category],
    all_cats: &[Category],
) -> Markup {
    let cats_json: String =
        serde_json::to_string(&tmpl_cats.iter().map(|c| &c.name).collect::<Vec<_>>())
            .unwrap_or_else(|_| "[]".to_string());
    let all_cats_json: String =
        serde_json::to_string(&all_cats.iter().map(|c| &c.name).collect::<Vec<_>>())
            .unwrap_or_else(|_| "[]".to_string());

    html! {
        // OOB swap to keep header name in sync
        h1 id="template-name" hx-swap-oob="true"
           class="font-serif-heading text-xl font-medium tracking-tight" style="color: var(--ink)" {
            (tmpl.name)
        }
        form hx-post=(format!("/admin/plan-templates/{}/meta", tmpl.id))
             hx-target="#template-meta"
             hx-swap="innerHTML"
             class="space-y-3" {
            div class="grid grid-cols-1 sm:grid-cols-2 gap-3" {
                div {
                    label class="block font-mono-stat text-[10px] tracking-widest uppercase mb-1" style="color: var(--muted)" for="tmpl-name" { "Name" }
                    input type="text" name="name" id="tmpl-name" required
                          class="w-full rounded border px-3 py-2 text-sm"
                          style="border-color: var(--rule); background: var(--paper); color: var(--ink)"
                          value=(tmpl.name);
                }
                // Category picker
                (maud::PreEscaped(r#"<script>
                  window.__catPicker = function(sel, all) {
                    return {
                      open: false, search: '', selected: sel, all: all,
                      get filtered() { return this.all.filter(c => !this.selected.includes(c) && c.includes(this.search.toLowerCase())) },
                      get canAddNew() { let v = this.search.trim().toLowerCase(); return v && !this.all.includes(v) && !this.selected.includes(v) },
                      get showEmpty() { return !this.filtered.length && !this.canAddNew },
                      add(c) { this.selected.push(c); this.search = ''; },
                      remove(c) { this.selected = this.selected.filter(s => s !== c); },
                      addNew() { let v = this.search.trim().toLowerCase(); if (v && !this.selected.includes(v)) { this.selected.push(v); this.search = ''; } },
                      backspace() { if (!this.search && this.selected.length) this.remove(this.selected[this.selected.length - 1]); }
                    }
                  }
                </script>"#))
                div "x-data"=(format!("__catPicker({cats_json}, {all_cats_json})")) {
                    label class="block font-mono-stat text-[10px] tracking-widest uppercase mb-1" style="color: var(--muted)" { "Categories" }
                    input type="hidden" name="categories" ":value"="selected.join(',')";
                    // Tag input — pills + text input share the same bordered row
                    div class="relative" {
                        div class="flex flex-wrap items-center gap-1 rounded border px-2 py-1.5"
                             style="border-color: var(--rule); background: var(--paper); min-height: 2.25rem"
                             "@click"="$refs.catInput.focus()" {
                            template "x-for"="cat in selected" ":key"="cat" {
                                span class="inline-flex items-center gap-1 font-mono-stat text-[10px] px-1.5 py-px rounded"
                                     style="color: var(--ink-2); background: var(--paper-2); border: 1px solid var(--rule)" {
                                    span x-text="cat" {}
                                    button type="button" class="hover:opacity-60 cursor-pointer"
                                           style="background: none; border: none; color: var(--muted); font-size: 12px; line-height: 1; padding: 0"
                                           "@click.stop"="remove(cat)" { "×" }
                                }
                            }
                            input type="text" autocomplete="off"
                                  class="flex-1 text-sm outline-none"
                                  style="background: transparent; color: var(--ink); border: none; min-width: 4rem; padding: 0"
                                  placeholder="Add..."
                                  "x-model"="search"
                                  "x-ref"="catInput"
                                  "@focus"="open = true"
                                  "@click.outside"="open = false"
                                  "@keydown.enter.prevent"="addNew()"
                                  "@keydown.backspace"="backspace()";
                        }
                        // Dropdown
                        div x-show="open" x-cloak
                            class="absolute z-10 mt-1 w-full rounded border shadow-lg overflow-y-auto"
                            style="background: var(--paper); border-color: var(--rule); max-height: 12rem" {
                            template "x-for"="cat in filtered" ":key"="cat" {
                                button type="button"
                                       class="w-full text-left px-3 py-1.5 text-sm cursor-pointer hover:opacity-80"
                                       style="background: transparent; border: none; color: var(--ink)"
                                       "@click"="add(cat); open = false" {
                                    span x-text="cat" {}
                                }
                            }
                            // "+ add" option
                            button type="button"
                                   x-show="canAddNew"
                                   class="w-full text-left px-3 py-1.5 text-sm cursor-pointer hover:opacity-80"
                                   style="background: transparent; border: none; color: var(--accent)"
                                   "@click"="addNew(); open = false" {
                                span { "+" }
                                " add \""
                                span x-text="search.trim()" {}
                                "\""
                            }
                            p x-show="showEmpty"
                              class="px-3 py-1.5 text-xs" style="color: var(--muted)" {
                                "No matching categories"
                            }
                        }
                    }
                }
            }
            div {
                label class="block font-mono-stat text-[10px] tracking-widest uppercase mb-1" style="color: var(--muted)" for="tmpl-desc" { "Description" }
                textarea name="description" id="tmpl-desc" rows="2"
                         class="w-full rounded border px-3 py-2 text-sm"
                         style="border-color: var(--rule); background: var(--paper); color: var(--ink)" {
                    (tmpl.description)
                }
            }
            button type="submit" class="btn-warm-ink text-xs py-1.5 px-3" { "Save details" }
        }
    }
}

// ── Import picker modal ──────────────────────────────────────────────

pub(crate) fn import_picker_modal(
    templates: &[PlanTemplate],
    practice_id: PracticeId,
    has_existing_timeline: bool,
) -> Markup {
    let close_js = "document.getElementById('template-picker-backdrop').remove(); document.getElementById('template-picker-modal').remove()";
    html! {
        // Backdrop
        div id="template-picker-backdrop"
            class="fixed inset-0 bg-black/40 z-40"
            onclick=(close_js) {}
        // Modal wrapper (centered)
        div id="template-picker-modal"
            class="fixed inset-0 z-50 flex items-start justify-center pt-12 px-4 pointer-events-none" {
        div class="bg-paper rounded-lg shadow-xl w-full max-w-3xl pointer-events-auto flex flex-col"
            style="max-height: calc(100vh - 8rem); min-height: min(36rem, calc(100vh - 8rem)); min-width: min(56rem, calc(100vw - 4rem))"
            "x-data"=(maud::PreEscaped(format!("{{ selected: {}, search: '', names: [{}] }}",
                if templates.is_empty() { "-1".to_string() } else { "0".to_string() },
                templates.iter().map(|t| format!("'{}'", t.name.to_lowercase().replace('\'', "\\'"))).collect::<Vec<_>>().join(","),
            ))) {
            // Header
            div class="flex items-center justify-between px-6 py-4 border-b" style="border-color: var(--rule)" {
                h2 class="font-serif-heading text-lg font-medium" style="color: var(--ink)" { "Use a template" }
                div class="flex items-center gap-3" {
                    @if !templates.is_empty() {
                        @for (i, tmpl) in templates.iter().enumerate() {
                            form method="post" action=(format!("/practices/{}/import-template", practice_id))
                                 class="inline"
                                 id=(format!("import-form-{i}"))
                                 x-show=(format!("selected === {i}")) {
                                input type="hidden" name="template_id" value=(tmpl.id);
                                button type="submit"
                                       class="btn-warm-ink text-xs py-1.5 px-4"
                                       hx-post=(format!("/practices/{}/import-template", practice_id))
                                       hx-target="#timeline-section"
                                       hx-swap="outerHTML"
                                       "hx-on::after-request"=(close_js)
                                       hx-confirm=(if has_existing_timeline { "Replace current practice plan with this template?" } else { "" }) {
                                    "Use"
                                }
                            }
                        }
                    }
                    button class="btn-warm-ghost text-xs py-1.5 px-3"
                           onclick=(close_js) {
                        "Cancel"
                    }
                }
            }
            @if templates.is_empty() {
                div class="px-6 py-8" {
                    p class="text-sm" style="color: var(--muted)" { "No templates available. Create one in Admin → Plan Templates." }
                }
            } @else {
                // Two-pane layout
                div class="flex flex-1 min-h-0" {
                    // LHS — search + template list
                    div class="w-2/5 border-r flex flex-col" style="border-color: var(--rule)" {
                        div class="px-3 py-2 border-b" style="border-color: var(--rule)" {
                            input type="text" placeholder="Search templates..."
                                  class="w-full rounded border px-2 py-1.5 text-sm"
                                  style="border-color: var(--rule); background: var(--paper); color: var(--ink)"
                                  "x-model"="search";
                        }
                        div class="flex-1 overflow-y-auto" {
                            @for (i, tmpl) in templates.iter().enumerate() {
                                @let tl = tmpl.timeline();
                                @let planned = tl.as_ref().map(|t| t.planned_minutes()).unwrap_or(0.0);
                                button class="w-full text-left px-4 py-2.5 cursor-pointer"
                                       ":style"=(format!("(() => {{ let vis = !search || names[{i}].includes(search.toLowerCase()); let sel = selected === {i}; return vis ? (sel ? 'background: color-mix(in oklch, var(--accent) 12%, var(--paper)); border-left: 3px solid var(--accent)' : 'background: var(--paper); border-left: 3px solid transparent') : 'display: none' }})()"))
                                       "@click"=(format!("if (selected === {i}) {{ htmx.trigger(document.querySelector('#import-form-{i} button'), 'click') }} else {{ selected = {i} }}")) {
                                    div class="flex items-center justify-between" {
                                        span class="font-serif-heading font-medium text-sm" style="color: var(--ink)" {
                                            (tmpl.name)
                                        }
                                        span class="font-mono-stat text-[10px]" style="color: var(--muted)" {
                                            (format!("{:.0}", planned)) " min"
                                        }
                                    }
                                    @if !tmpl.description.is_empty() {
                                        p class="text-xs mt-0.5" style="color: var(--muted)" { (tmpl.description) }
                                    }
                                }
                            }
                        }
                    }
                    // RHS — preview of selected template
                    div class="w-3/5 overflow-y-auto p-4" {
                        @if has_existing_timeline {
                            p class="text-xs mb-3" style="color: var(--warn)" {
                                "This will replace the current practice plan."
                            }
                        }
                        @for (i, tmpl) in templates.iter().enumerate() {
                            @let tl = tmpl.timeline().unwrap_or_else(|| lineup_db::timeline::Timeline::default_empty(90));
                            div x-show=(format!("selected === {i}")) {
                                (super::timeline::preview(&tl))
                            }
                        }
                    }
                }
            }
        }
        }
    }
}
