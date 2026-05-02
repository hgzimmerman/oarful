//! Getting-started checklist shown on the Practices page for new tenants.

use maud::{html, Markup};

/// Progress state for each onboarding step.
pub(crate) struct OnboardingState {
    pub(crate) boat_count: usize,
    pub(crate) rower_count: usize,
    pub(crate) practice_count: usize,
    pub(crate) has_committed_lineup: bool,
}

impl OnboardingState {
    pub(crate) fn all_complete(&self) -> bool {
        self.boat_count > 0
            && self.rower_count > 0
            && self.practice_count > 0
            && self.has_committed_lineup
    }
}

pub(crate) fn onboarding_checklist(state: &OnboardingState) -> Markup {
    let steps: &[(bool, &str, &str, &str)] = &[
        (true, "Create your account", "", ""),
        (
            state.boat_count > 0,
            "Add boats to your fleet",
            "/admin/fleet",
            "Add the boats your club rows in.",
        ),
        (
            state.rower_count > 0,
            "Add rowers to your roster",
            "/team/roster",
            "Add rowers on the roster page. You can also bulk-import from a spreadsheet via the Sync tab.",
        ),
        (
            state.practice_count > 0,
            "Create your first practice",
            "",
            "Pick a date using the form above.",
        ),
        (
            state.has_committed_lineup,
            "Solve your first lineup",
            "",
            "Open a practice and run the solver to generate seat assignments.",
        ),
    ];

    let done_count = steps.iter().filter(|(done, _, _, _)| *done).count();
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
                       title="Dismiss" {
                    // X icon (inline SVG)
                    svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5" fill="none"
                        viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" {
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
                @for (done, label, href, subtitle) in steps {
                    li class="flex items-start gap-3 py-2" {
                        // Checkmark circle
                        @if *done {
                            div class="mt-0.5 w-5 h-5 rounded-full bg-emerald-500 flex items-center justify-center shrink-0" {
                                svg xmlns="http://www.w3.org/2000/svg" class="w-3 h-3 text-white" fill="none"
                                    viewBox="0 0 24 24" stroke="currentColor" stroke-width="3" {
                                    path stroke-linecap="round" stroke-linejoin="round"
                                         d="M5 13l4 4L19 7";
                                }
                            }
                        } @else {
                            div class="mt-0.5 w-5 h-5 rounded-full border-2 border-rule shrink-0" {}
                        }

                        div class="flex-1 min-w-0" {
                            @if !href.is_empty() && !done {
                                a href=(href)
                                  class="font-medium text-ink hover:text-ink-2 underline decoration-rule hover:decoration-ink transition" {
                                    (label)
                                }
                            } @else {
                                span class={ "font-medium " @if *done { "text-ink-3 line-through" } @else { "text-ink" } } {
                                    (label)
                                }
                            }
                            @if !subtitle.is_empty() && !done {
                                p class="text-sm text-ink-3 mt-0.5" { (subtitle) }
                            }
                        }
                    }
                }
            }
        }
    }
}
