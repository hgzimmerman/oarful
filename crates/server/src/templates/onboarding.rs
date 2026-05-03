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

pub(crate) fn onboarding_checklist(state: &OnboardingState) -> Markup {
    if state.is_dismissed() || state.all_complete() {
        return html! {};
    }

    let steps = &[
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
            subtitle: "Add rowers on the roster page. You can also bulk-import from a spreadsheet via the Sync tab.",
        },
        Step {
            done: state.completed.contains(&OnboardingStep::CustomizeRower),
            label: "Set rower attributes",
            href: "/team/roster",
            subtitle: "Set skill, strength, and side preference on your rowers for better lineups.",
        },
        Step {
            done: state.completed.contains(&OnboardingStep::CreatePractice),
            label: "Create your first practice",
            href: "",
            subtitle: "Pick a date using the form above.",
        },
        Step {
            done: state.completed.contains(&OnboardingStep::GenerateLineup),
            label: "Generate your first lineup",
            href: "",
            subtitle: "Open a practice and create seat assignments.",
        },
    ];

    let done_count = steps.iter().filter(|s| s.done).count();
    let total = steps.len();

    html! {
        div id="onboarding-checklist"
            class="bg-paper rounded-lg shadow-soft p-5 mb-6 border border-rule-2" {
            div class="flex items-center justify-between mb-4" {
                div {
                    h2 class="text-lg font-bold text-ink" { "Getting started" }
                    p class="text-sm text-ink-3 mt-0.5" {
                        (done_count) " of " (total) " complete"
                    }
                }
                button type="button"
                       hx-post="/onboarding/dismiss"
                       hx-target="#onboarding-checklist"
                       hx-swap="delete"
                       class="text-ink-3 hover:text-ink transition p-1"
                       "aria-label"="Dismiss getting started" {
                    // X icon (inline SVG)
                    svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5" fill="none"
                        viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"
                        "aria-hidden"="true" {
                        path stroke-linecap="round" stroke-linejoin="round"
                             d="M6 18L18 6M6 6l12 12";
                    }
                }
            }

            // Progress bar
            div class="w-full bg-paper-2 rounded-full h-1.5 mb-4" {
                div class="bg-emerald-500 h-1.5 rounded-full transition-all"
                    style={ "width: " (done_count * 100 / total) "%" } {}
            }

            ol class="space-y-1" {
                @for step in steps {
                    li class="flex items-start gap-3 py-2" {
                        // Checkmark circle
                        @if step.done {
                            div class="mt-0.5 w-5 h-5 rounded-full bg-emerald-500 flex items-center justify-center shrink-0"
                                "aria-hidden"="true" {
                                svg xmlns="http://www.w3.org/2000/svg" class="w-3 h-3 text-white" fill="none"
                                    viewBox="0 0 24 24" stroke="currentColor" stroke-width="3" {
                                    path stroke-linecap="round" stroke-linejoin="round"
                                         d="M5 13l4 4L19 7";
                                }
                            }
                        } @else {
                            div class="mt-0.5 w-5 h-5 rounded-full border-2 border-rule shrink-0"
                                "aria-hidden"="true" {}
                        }

                        div class="flex-1 min-w-0" {
                            @if !step.href.is_empty() && !step.done {
                                a href=(step.href)
                                  class="font-medium text-ink hover:text-ink-2 underline decoration-rule hover:decoration-ink transition" {
                                    (step.label)
                                }
                            } @else {
                                span class={ "font-medium " @if step.done { "text-ink-3 line-through" } @else { "text-ink" } } {
                                    (step.label)
                                }
                            }
                            @if !step.subtitle.is_empty() && !step.done {
                                p class="text-sm text-ink-3 mt-0.5" { (step.subtitle) }
                            }
                        }
                    }
                }
            }
        }
    }
}
