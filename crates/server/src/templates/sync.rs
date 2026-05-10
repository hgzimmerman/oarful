//! Sync-sheet form + result panel.

use chrono::NaiveDateTime;
use lineup_sheets::SyncSummary;
use maud::{html, Markup};

use crate::handlers::sync::SyncFormInput;

pub(crate) fn form_content(
    prev: Option<&SyncFormInput>,
    summary: Option<&SyncSummary>,
    error: Option<&str>,
    last_synced: Option<NaiveDateTime>,
) -> Markup {
    let id_value = prev.map(|p| p.spreadsheet_id.as_str()).unwrap_or("");
    let gid_value = prev.map(|p| p.gid).unwrap_or(0);

    html! {
        // ── Header ──
        header class="border-b px-4 sm:px-8 py-3 sm:py-4" style="border-color: var(--rule); background: var(--paper)" {
            h1 class="font-serif-heading text-2xl font-medium tracking-tight" style="color: var(--ink)" {
                "Sync sheet"
            }
            p class="font-mono-stat text-xs tracking-wide mt-1" style="color: var(--muted)" {
                "Pull roster and availability from a publicly-shared Google Sheet."
            }
        }

        div class="px-4 sm:px-8 py-6 max-w-3xl mx-auto space-y-6" {
            @if let Some(ts) = last_synced {
                div class="flex items-center gap-2 font-mono-stat text-[10px] tracking-wide" style="color: var(--muted)" {
                    span class="uppercase" { "Last synced" }
                    time class="font-semibold" style="color: var(--ink-2)"
                         datetime=(ts.format("%Y-%m-%dT%H:%M:%S").to_string()) {
                        (ts.format("%b %d, %Y at %H:%M UTC").to_string())
                    }
                }
            }
            @if let Some(msg) = error {
                div class="bg-bad/10 border-l-4 border-bad px-4 py-3 rounded text-sm text-ink" {
                    strong { "Error. " } (msg)
                }
            }

            form method="post" action="/sync"
                 hx-post="/sync" hx-disabled-elt="find button"
                 hx-target="#content"
                 hx-push-url="false"
                 hx-indicator="#sync-spinner"
                 class="rounded-lg p-6 space-y-4" style="background: var(--paper); box-shadow: var(--shadow-soft)" {
                (format_help())
                div {
                    label for="spreadsheet_id" class="block font-mono-stat text-[10px] tracking-widest uppercase font-semibold mb-1.5" style="color: var(--ink-2)" {
                        "Spreadsheet ID"
                    }
                    input id="spreadsheet_id" name="spreadsheet_id" type="text" required
                          value=(id_value)
                          placeholder="1AbCDeFgHiJkLmNoPqRsTuVwXyZ..."
                          class="w-full border border-rule rounded px-3 py-2 font-mono text-sm focus:border-ink-3 focus:outline-none";
                    p class="text-xs mt-1" style="color: var(--muted)" {
                        "From the sheet URL: "
                        code class="px-1 rounded" style="background: var(--paper-2)" { "/spreadsheets/d/" strong { "ID" } "/edit" }
                        ". The sheet must be set to 'Anyone with the link can view'."
                    }
                }
                div {
                    label for="gid" class="block font-mono-stat text-[10px] tracking-widest uppercase font-semibold mb-1.5" style="color: var(--ink-2)" {
                        "Tab ID (gid)"
                    }
                    input id="gid" name="gid" type="number" min="0" value=(gid_value)
                          class="w-32 border border-rule rounded px-3 py-2 font-mono text-sm focus:border-ink-3 focus:outline-none";
                    p class="text-xs mt-1" style="color: var(--muted)" {
                        "0 for the first tab. Otherwise the "
                        code class="px-1 rounded" style="background: var(--paper-2)" { "gid=" }
                        " value in the tab's URL."
                    }
                }
                div {
                    @let current_filter = prev.map(|p| p.row_filter.as_str()).unwrap_or("");
                    label for="row_filter" class="block font-mono-stat text-[10px] tracking-widest uppercase font-semibold mb-1.5" style="color: var(--ink-2)" {
                        "Row filter"
                    }
                    select id="row_filter" name="row_filter" required
                           class="w-48 border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none" {
                        @if current_filter.is_empty() {
                            option value="" disabled selected { "Select…" }
                        }
                        option value="All" selected[current_filter == "All"] { "All rows" }
                        option value="Sweep" selected[current_filter == "Sweep"] { "Sweep only" }
                        option value="Sculling" selected[current_filter == "Sculling"] { "Sculling only" }
                    }
                    p class="text-xs mt-1" style="color: var(--muted)" {
                        "Which rows to import. Must be set before syncing."
                    }
                }
                div {
                    label for="poll_interval_minutes" class="block font-mono-stat text-[10px] tracking-widest uppercase font-semibold mb-1.5" style="color: var(--ink-2)" {
                        "Auto-sync interval (minutes)"
                    }
                    input id="poll_interval_minutes" name="poll_interval_minutes" type="number"
                          min="0" step="1"
                          value=(prev.and_then(|p| p.poll_interval_minutes).map(|m| m.to_string()).unwrap_or_default())
                          placeholder="Off"
                          class="w-32 border border-rule rounded px-3 py-2 font-mono text-sm focus:border-ink-3 focus:outline-none";
                    p class="text-xs mt-1" style="color: var(--muted)" {
                        "Leave empty or 0 to disable. When set, the sheet is re-synced automatically on this interval."
                    }
                }
                div class="flex items-center space-x-3" {
                    button type="submit" class="btn-warm-ink py-2 px-5" {
                        "Sync"
                    }
                    span #sync-spinner class="htmx-indicator font-mono-stat text-xs" style="color: var(--muted)" {
                        "Fetching and syncing…"
                    }
                }
            }

            @if let Some(s) = summary {
                (summary_panel(s))
            }
        }
    }
}

fn format_help() -> Markup {
    html! {
        div x-data="{ showHelp: false }" {
            button type="button"
                   "@click"="showHelp = !showHelp"
                   ":aria-expanded"="showHelp"
                   class="flex items-center gap-1.5 font-mono-stat text-[10px] tracking-wide transition" style="color: var(--muted)" {
                span class="inline-flex items-center justify-center w-4 h-4 rounded-full text-[10px] font-bold" style="border: 1px solid var(--rule); color: var(--ink-3)" { "?" }
                " Expected sheet format"
            }
            div x-show="showHelp" x-cloak class="mt-3 rounded p-4 text-sm space-y-3" style="border: 1px solid var(--rule-2); color: var(--ink-2)" {
                p { "The row containing a " strong { "Sweep/Scull" } " column is treated as the header. Columns are matched by name (case-insensitive) and can appear in any order. Rows above the header are ignored." }
                div class="overflow-x-auto" {
                    table class="text-xs font-mono border-collapse w-full" {
                        caption class="sr-only" { "Expected spreadsheet columns" }
                        thead {
                            tr style="background: var(--paper-2)" {
                                th scope="col" class="px-2 py-1" style="border: 1px solid var(--rule-2)" { "Sweep/Scull" }
                                th scope="col" class="px-2 py-1" style="border: 1px solid var(--rule-2)" { "Last Name" }
                                th scope="col" class="px-2 py-1" style="border: 1px solid var(--rule-2)" { "First Name" }
                                th scope="col" class="px-2 py-1" style="border: 1px solid var(--rule-2)" { "Email" }
                                th scope="col" class="px-2 py-1" style="border: 1px solid var(--rule-2)" { "Side/Cox" }
                                th scope="col" class="px-2 py-1" style="border: 1px solid var(--rule-2)" { "3/30" }
                                th scope="col" class="px-2 py-1" style="border: 1px solid var(--rule-2)" { "4/1" }
                            }
                        }
                        tbody {
                            tr {
                                td class="px-2 py-1" style="border: 1px solid var(--rule-2)" { "Sweep" }
                                td class="px-2 py-1" style="border: 1px solid var(--rule-2)" { "Smith" }
                                td class="px-2 py-1" style="border: 1px solid var(--rule-2)" { "Alice" }
                                td class="px-2 py-1" style="border: 1px solid var(--rule-2)" { "alice@example.com" }
                                td class="px-2 py-1" style="border: 1px solid var(--rule-2)" { "Port" }
                                td class="px-2 py-1" style="border: 1px solid var(--rule-2)" { "Attending" }
                                td class="px-2 py-1" style="border: 1px solid var(--rule-2)" {}
                            }
                            tr {
                                td class="px-2 py-1" style="border: 1px solid var(--rule-2)" { "Sculling" }
                                td class="px-2 py-1" style="border: 1px solid var(--rule-2)" { "Jones" }
                                td class="px-2 py-1" style="border: 1px solid var(--rule-2)" { "Bob" }
                                td class="px-2 py-1" style="border: 1px solid var(--rule-2)" { "bob@example.com" }
                                td class="px-2 py-1" style="border: 1px solid var(--rule-2)" { "Either" }
                                td class="px-2 py-1" style="border: 1px solid var(--rule-2)" { "Not Attending" }
                                td class="px-2 py-1" style="border: 1px solid var(--rule-2)" { "Attending" }
                            }
                        }
                    }
                }
                ul class="text-xs space-y-1 list-disc pl-4" style="color: var(--muted)" {
                    li { "Required columns: " strong { "Sweep/Scull" } ", " strong { "Last Name" } ", " strong { "First Name" } ", " strong { "Email" } ", " strong { "Side/Cox" } "." }
                    li { "Optional columns (ignored): Pronoun, Can you Scull?, or any other unrecognized column." }
                    li { "Date columns use " strong { "M/D" } " format (e.g. 3/30, 11/5). Year is inferred from the current season." }
                    li { "Availability values: " strong { "Attending" } ", " strong { "Not Attending" } ", or empty (no response)." }
                    li { "Side/Cox values: " strong { "Port" } ", " strong { "Starboard" } ", " strong { "Either" } ", or " strong { "Cox" } " (designated coxswain)." }
                    li { "Rowers are matched by " strong { "email" } ". Rows without an email are skipped." }
                }
            }
        }
    }
}

fn summary_panel(s: &SyncSummary) -> Markup {
    html! {
        section class="rounded-lg p-6 space-y-4" style="background: var(--paper); box-shadow: var(--shadow-soft)" {
            h2 class="font-serif-heading text-xl font-medium tracking-tight" style="color: var(--ink)" { "Sync complete" }

            div class="grid grid-cols-2 md:grid-cols-3 gap-3" {
                (stat("Rows read", s.rows_read))
                (stat("Sweep", s.sweep_rows))
                (stat("Sculling", s.sculling_rows))
                (stat("Members created", s.rowers_created))
                (stat("Members updated", s.rowers_updated))
                (stat("Availability upserted", s.availabilities_upserted))
            }

            @if s.rows_skipped_no_email > 0 {
                p class="font-mono-stat text-[10px] tracking-wide" style="color: var(--muted)" {
                    "Skipped " (s.rows_skipped_no_email) " row(s) without an email."
                }
            }

            @if !s.warnings.is_empty() {
                div {
                    h3 class="font-mono-stat text-[10px] tracking-widest uppercase font-semibold mb-2" style="color: var(--warn)" {
                        "Warnings (" (s.warnings.len()) ")"
                    }
                    ul class="list-disc pl-5 space-y-1 text-sm" style="color: var(--warn)" {
                        @for w in &s.warnings {
                            li { (w) }
                        }
                    }
                }
            }

            div class="pt-3" style="border-top: 1px solid var(--rule-2)" {
                a href="/practices"
                  hx-get="/practices"
                  hx-target="#content"
                  hx-swap="innerHTML transition:true"
                  hx-push-url="true"
                  class="font-mono-stat text-xs font-semibold hover:underline" style="color: var(--accent)" {
                    "→ View practices dashboard"
                }
            }
        }
    }
}

fn stat(label: &str, value: usize) -> Markup {
    html! {
        div class="rounded p-3" style="background: var(--paper-2)" {
            div class="font-mono-stat text-[9px] tracking-widest uppercase font-semibold" style="color: var(--muted)" { (label) }
            div class="cv-stat-num font-serif-heading mt-0.5" { (value) }
        }
    }
}
