//! Getting-started checklist shown on the Practices page for coaches/PDs.

use std::collections::HashSet;

use lineup_db::onboarding::OnboardingStep;
use maud::{html, Markup};

/// Per-user onboarding progress, built from their completed steps.
pub(crate) struct OnboardingState {
    pub(crate) completed: HashSet<OnboardingStep>,
}

impl OnboardingState {
    pub(crate) fn is_dismissed(&self) -> bool {
        self.completed.contains(&OnboardingStep::Dismissed)
    }

    fn all_complete(&self) -> bool {
        use OnboardingStep::*;
        [
            AddBoats,
            AddRowers,
            CustomizeRower,
            CreatePractice,
            GenerateLineup,
        ]
        .iter()
        .all(|s| self.completed.contains(s))
    }
}

struct Step {
    done: bool,
    label: &'static str,
    href: &'static str,
    subtitle: &'static str,
}

/// Bell icon for the navbar with a badge showing remaining steps.
/// Clicking opens an absolutely-positioned dropdown with the checklist.
/// Returns empty markup if dismissed or all complete.
pub(crate) fn onboarding_bell(state: &OnboardingState) -> Markup {
    if state.is_dismissed() || state.all_complete() {
        return html! {};
    }

    let steps = checklist_steps(state);
    let done_count = steps.iter().filter(|s| s.done).count();
    let total = steps.len();
    let remaining = total - done_count;

    html! {
        div class="relative" "x-data"="{ open: false }" {
            // Bell button with badge
            button type="button"
                   "@click"="open = !open"
                   class="relative p-2 rounded hover:bg-paper/10 transition"
                   aria-label="Setup tasks" {
                // Bell SVG
                svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5" fill="none"
                    viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"
                    "aria-hidden"="true" {
                    path stroke-linecap="round" stroke-linejoin="round"
                         d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9";
                }
                // Badge
                span class="absolute -top-0.5 -right-0.5 bg-red-500 text-white text-[10px] font-bold rounded-full w-4 h-4 flex items-center justify-center" {
                    (remaining)
                }
            }

            // Dropdown panel
            div class="absolute right-0 top-full mt-2 w-80 bg-paper text-ink rounded-lg shadow-xl border border-rule z-50"
                "x-show"="open"
                "@click.outside"="open = false"
                "x-transition" {
                div class="px-4 pt-3 pb-2 flex items-center justify-between border-b border-rule-2" {
                    div {
                        h3 class="text-sm font-bold text-ink" { "Getting started" }
                        p class="text-xs text-ink-3" {
                            (done_count) " of " (total) " complete"
                        }
                    }
                    button type="button"
                           hx-post="/onboarding/dismiss"
                           hx-target="#nav-onboarding"
                           hx-swap="innerHTML"
                           "@click"="open = false"
                           class="text-ink-3 hover:text-ink transition p-1 text-xs underline" {
                        "Dismiss"
                    }
                }

                // Progress bar
                div class="px-4 pt-2" {
                    div class="w-full bg-paper-2 rounded-full h-1" {
                        div class="bg-emerald-500 h-1 rounded-full transition-all"
                            style={ "width: " (done_count * 100 / total) "%" } {}
                    }
                }

                ol class="px-4 py-2 space-y-0.5" {
                    @for step in &steps {
                        li class="flex items-start gap-2 py-1.5" {
                            @if step.done {
                                div class="mt-0.5 w-4 h-4 rounded-full bg-emerald-500 flex items-center justify-center shrink-0" {
                                    svg xmlns="http://www.w3.org/2000/svg" class="w-2.5 h-2.5 text-white" fill="none"
                                        viewBox="0 0 24 24" stroke="currentColor" stroke-width="3" {
                                        path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7";
                                    }
                                }
                            } @else {
                                div class="mt-0.5 w-4 h-4 rounded-full border-2 border-rule shrink-0" {}
                            }
                            div class="min-w-0" {
                                @if !step.href.is_empty() && !step.done {
                                    a href=(step.href)
                                      class="text-sm font-medium text-ink hover:underline"
                                      hx-get=(step.href)
                                      hx-target="#content"
                                      hx-swap="innerHTML transition:true"
                                      hx-push-url="true"
                                      "@click"="open = false" {
                                        (step.label)
                                    }
                                } @else {
                                    span class={"text-sm font-medium " @if step.done { "text-ink-3 line-through" } @else { "text-ink" }} {
                                        (step.label)
                                    }
                                }
                                @if !step.subtitle.is_empty() && !step.done {
                                    p class="text-xs text-ink-3 mt-0.5" { (step.subtitle) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn checklist_steps(state: &OnboardingState) -> Vec<Step> {
    vec![
        Step {
            done: state.completed.contains(&OnboardingStep::AddBoats),
            label: "Add boats to your fleet",
            href: "/admin/fleet",
            subtitle: "Add the boats your club rows in.",
        },
        Step {
            done: state.completed.contains(&OnboardingStep::AddRowers),
            label: "Add rowers to your roster",
            href: "/team/roster",
            subtitle: "Add rowers or bulk-import from a spreadsheet.",
        },
        Step {
            done: state.completed.contains(&OnboardingStep::CustomizeRower),
            label: "Set rower attributes",
            href: "/team/roster",
            subtitle: "Set skill, strength, and side preference.",
        },
        Step {
            done: state.completed.contains(&OnboardingStep::CreatePractice),
            label: "Create your first practice",
            href: "/practices",
            subtitle: "Pick a date on the practices page.",
        },
        Step {
            done: state.completed.contains(&OnboardingStep::GenerateLineup),
            label: "Generate your first lineup",
            href: "",
            subtitle: "Open a practice and create seat assignments.",
        },
    ]
}
