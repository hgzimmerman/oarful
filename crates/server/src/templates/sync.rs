//! Sync-sheet form + result panel.

use chrono::NaiveDateTime;
use lineup_sheets::SyncSummary;
use maud::{html, Markup};

use super::layout::page_header;
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
        (page_header(
            "Sync sheet",
            Some("Pull roster and availability from a publicly-shared Google Sheet."),
        ))
        (format_help())
        div class="px-4 sm:px-8 py-6 max-w-3xl mx-auto space-y-6" {
            @if let Some(ts) = last_synced {
                div class="text-xs text-ink-3" {
                    "Last synced: "
                    time datetime=(ts.format("%Y-%m-%dT%H:%M:%S").to_string()) {
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
                 hx-post="/sync"
                 hx-target="#content"
                 hx-push-url="false"
                 hx-indicator="#sync-spinner"
                 class="bg-paper rounded-lg shadow-soft p-6 space-y-4" {
                div {
                    label for="spreadsheet_id" class="block text-sm font-semibold text-ink-2 mb-1" {
                        "Spreadsheet ID"
                    }
                    input id="spreadsheet_id" name="spreadsheet_id" type="text" required
                          value=(id_value)
                          placeholder="1AbCDeFgHiJkLmNoPqRsTuVwXyZ..."
                          class="w-full border border-rule rounded px-3 py-2 font-mono text-sm focus:border-ink-3 focus:outline-none";
                    p class="text-xs text-ink-3 mt-1" {
                        "From the sheet URL: "
                        code class="bg-paper-2 px-1" { "/spreadsheets/d/" strong { "ID" } "/edit" }
                        ". The sheet must be set to 'Anyone with the link can view'."
                    }
                }
                div {
                    label for="gid" class="block text-sm font-semibold text-ink-2 mb-1" {
                        "Tab ID (gid)"
                    }
                    input id="gid" name="gid" type="number" min="0" value=(gid_value)
                          class="w-32 border border-rule rounded px-3 py-2 font-mono text-sm focus:border-ink-3 focus:outline-none";
                    p class="text-xs text-ink-3 mt-1" {
                        "0 for the first tab. Otherwise the "
                        code class="bg-paper-2 px-1" { "gid=" }
                        " value in the tab's URL."
                    }
                }
                div {
                    @let current_filter = prev.map(|p| p.row_filter.as_str()).unwrap_or("");
                    label for="row_filter" class="block text-sm font-semibold text-ink-2 mb-1" {
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
                    p class="text-xs text-ink-3 mt-1" {
                        "Which rows to import. Must be set before syncing."
                    }
                }
                div {
                    label for="poll_interval_minutes" class="block text-sm font-semibold text-ink-2 mb-1" {
                        "Auto-sync interval (minutes)"
                    }
                    input id="poll_interval_minutes" name="poll_interval_minutes" type="number"
                          min="0" step="1"
                          value=(prev.and_then(|p| p.poll_interval_minutes).map(|m| m.to_string()).unwrap_or_default())
                          placeholder="Off"
                          class="w-32 border border-rule rounded px-3 py-2 font-mono text-sm focus:border-ink-3 focus:outline-none";
                    p class="text-xs text-ink-3 mt-1" {
                        "Leave empty or 0 to disable. When set, the sheet is re-synced automatically on this interval."
                    }
                }
                div class="flex items-center space-x-3" {
                    button type="submit"
                           class="bg-good hover:opacity-90 text-paper font-semibold px-4 py-2 rounded shadow-soft transition" {
                        "Sync"
                    }
                    span #sync-spinner class="htmx-indicator text-sm text-ink-3" {
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
        div class="px-4 sm:px-8 max-w-3xl mx-auto"
            x-data="{ showHelp: false }" {
            button type="button"
                   "@click"="showHelp = !showHelp"
                   ":aria-expanded"="showHelp"
                   class="text-xs text-ink-3 hover:text-ink-2 transition flex items-center gap-1" {
                span class="inline-flex items-center justify-center w-4 h-4 rounded-full border border-current text-[10px] font-bold" { "?" }
                " Expected sheet format"
            }
            div x-show="showHelp" x-cloak class="mt-3 bg-paper rounded-lg shadow-soft p-4 text-sm text-ink-2 space-y-3" {
                p { "The first row that starts with " strong { "Sweep/Scull" } " is treated as the header. Rows above it are ignored." }
                div class="overflow-x-auto" {
                    table class="text-xs font-mono border-collapse w-full" {
                        caption class="sr-only" { "Expected spreadsheet columns" }
                        thead {
                            tr class="bg-paper-2 text-left" {
                                th scope="col" class="px-2 py-1 border border-rule-2" { "Sweep/Scull" }
                                th scope="col" class="px-2 py-1 border border-rule-2" { "Last Name" }
                                th scope="col" class="px-2 py-1 border border-rule-2" { "First Name" }
                                th scope="col" class="px-2 py-1 border border-rule-2 text-ink-3" { "Pronoun" }
                                th scope="col" class="px-2 py-1 border border-rule-2" { "Email" }
                                th scope="col" class="px-2 py-1 border border-rule-2 text-ink-3" { "(unused)" }
                                th scope="col" class="px-2 py-1 border border-rule-2" { "Side/Cox" }
                                th scope="col" class="px-2 py-1 border border-rule-2" { "3/30" }
                                th scope="col" class="px-2 py-1 border border-rule-2" { "4/1" }
                            }
                        }
                        tbody {
                            tr {
                                td class="px-2 py-1 border border-rule-2" { "Sweep" }
                                td class="px-2 py-1 border border-rule-2" { "Smith" }
                                td class="px-2 py-1 border border-rule-2" { "Alice" }
                                td class="px-2 py-1 border border-rule-2 text-ink-3" { "she/her" }
                                td class="px-2 py-1 border border-rule-2" { "alice@example.com" }
                                td class="px-2 py-1 border border-rule-2 text-ink-3" {}
                                td class="px-2 py-1 border border-rule-2" { "Port" }
                                td class="px-2 py-1 border border-rule-2" { "Attending" }
                                td class="px-2 py-1 border border-rule-2" {}
                            }
                            tr {
                                td class="px-2 py-1 border border-rule-2" { "Sculling" }
                                td class="px-2 py-1 border border-rule-2" { "Jones" }
                                td class="px-2 py-1 border border-rule-2" { "Bob" }
                                td class="px-2 py-1 border border-rule-2 text-ink-3" {}
                                td class="px-2 py-1 border border-rule-2" { "bob@example.com" }
                                td class="px-2 py-1 border border-rule-2 text-ink-3" {}
                                td class="px-2 py-1 border border-rule-2" { "Either" }
                                td class="px-2 py-1 border border-rule-2" { "Not Attending" }
                                td class="px-2 py-1 border border-rule-2" { "Attending" }
                            }
                        }
                    }
                }
                ul class="text-xs text-ink-3 space-y-1 list-disc pl-4" {
                    li { "Columns must be in this order. Column headers are ignored except for " strong { "Sweep/Scull" } " (marks the header row) and dates." }
                    li { "Date columns use " strong { "M/D" } " format (e.g. 3/30, 11/5). Year is inferred from the current season." }
                    li { "Availability values: " strong { "Attending" } ", " strong { "Not Attending" } ", or empty (no response)." }
                    li { "Side/Cox values: " strong { "Port" } ", " strong { "Starboard" } ", " strong { "Either" } ", or " strong { "Cox" } " (designated coxswain)." }
                    li { "Rowers are matched by " strong { "email" } ". Rows without an email are skipped." }
                    li { "The Pronoun and column 5 are read but not stored." }
                }
            }
        }
    }
}

fn summary_panel(s: &SyncSummary) -> Markup {
    html! {
        section class="bg-paper rounded-lg shadow-soft p-6 space-y-4" {
            h2 class="text-xl font-bold text-ink" { "Sync complete" }

            div class="grid grid-cols-2 md:grid-cols-3 gap-3 text-sm" {
                (stat("Rows read", s.rows_read))
                (stat("Sweep", s.sweep_rows))
                (stat("Sculling", s.sculling_rows))
                (stat("Members created", s.rowers_created))
                (stat("Members updated", s.rowers_updated))
                (stat("Availability upserted", s.availabilities_upserted))
            }

            @if s.rows_skipped_no_email > 0 {
                p class="text-xs text-ink-3" {
                    "Skipped " (s.rows_skipped_no_email) " row(s) without an email."
                }
            }

            @if !s.warnings.is_empty() {
                div {
                    h3 class="font-semibold text-ink-2 mb-2" {
                        "Warnings (" (s.warnings.len()) ")"
                    }
                    ul class="list-disc pl-5 space-y-1 text-sm text-warn" {
                        @for w in &s.warnings {
                            li { (w) }
                        }
                    }
                }
            }

            div class="pt-2 border-t border-rule-2" {
                a href="/practices"
                  hx-get="/practices"
                  hx-target="#content"
                  hx-push-url="true"
                  class="text-emerald-700 hover:text-emerald-900 font-semibold text-sm" {
                    "→ View practices dashboard"
                }
            }
        }
    }
}

fn stat(label: &str, value: usize) -> Markup {
    html! {
        div class="bg-paper rounded p-3" {
            div class="text-xs text-ink-3 uppercase tracking-wide" { (label) }
            div class="text-2xl font-bold text-ink" { (value) }
        }
    }
}
