//! Solver profile editor modal — create, edit, duplicate presets.

use lineup_solver::SolverConfig;
use maud::{html, Markup};

/// Weight group for organizing sliders in the modal.
struct WeightGroup {
    label: &'static str,
    fields: Vec<WeightField>,
}

struct WeightField {
    name: &'static str,
    label: &'static str,
    value: i32,
    min: i32,
    max: i32,
    help: &'static str,
}

fn weight_groups(cfg: &SolverConfig) -> Vec<WeightGroup> {
    vec![
        WeightGroup {
            label: "Core",
            fields: vec![
                WeightField { name: "skill_variance_weight", label: "Skill variance", value: cfg.skill_variance_weight, min: 0, max: 10, help: "Even out skill across boats. High = similar boats. Zero = skill gaps OK." },
                WeightField { name: "pair_affinity_weight", label: "Pair affinity", value: cfg.pair_affinity_weight, min: 0, max: 10, help: "Keep paired rowers in the same boat. Zero = ignore pair preferences." },
                WeightField { name: "seat_affinity_weight", label: "Seat affinity", value: cfg.seat_affinity_weight, min: 0, max: 10, help: "Honor rowers' preferred seats. Zero = seat preferences ignored." },
                WeightField { name: "side_preference_weight", label: "Side preference", value: cfg.side_preference_weight, min: 0, max: 10, help: "Avoid wrong-side placement. Zero = side doesn't matter." },
                WeightField { name: "weight_class_slack_weight", label: "Weight class", value: cfg.weight_class_slack_weight, min: 0, max: 10, help: "Match rower weight class to boat. High = strict, zero = ignore." },
            ],
        },
        WeightGroup {
            label: "Placement",
            fields: vec![
                WeightField { name: "placement_reward_weight", label: "Placement reward", value: cfg.placement_reward_weight, min: 0, max: 10, help: "Pressure to field boats over benching. Too low = empty boats. Too high = ignores quality." },
                WeightField { name: "partial_fill_bonus", label: "Partial fill", value: cfg.partial_fill_bonus, min: 0, max: 10, help: "Fill optional seats when partial fill is on. Zero = leave them empty." },
                WeightField { name: "minimize_bench_weight", label: "Minimize bench", value: cfg.minimize_bench_weight, min: 0, max: 10, help: "Avoid benching anyone. High = everyone rows. Low = OK to bench for quality." },
                WeightField { name: "non_scull_retention_weight", label: "Sweep-only retention", value: cfg.non_scull_retention_weight, min: 0, max: 10, help: "Extra pressure to place rowers who can't scull. They have nowhere else to go." },
            ],
        },
        WeightGroup {
            label: "Boat structure",
            fields: vec![
                WeightField { name: "pair_strength_weight", label: "Seat-pair strength", value: cfg.pair_strength_weight, min: 0, max: 10, help: "Match strength in adjacent seats (1-2, 3-4, ...). High = balanced pairs." },
                WeightField { name: "bow_pair_strength_weight", label: "Bow pair strength", value: cfg.bow_pair_strength_weight, min: 0, max: 10, help: "Extra balance for seats 1-2. Bow pair affects set and steering most." },
                WeightField { name: "height_balance_weight", label: "Height balance", value: cfg.height_balance_weight, min: 0, max: 10, help: "Match heights in adjacent seats. Gentle preference, not critical." },
                WeightField { name: "end_pair_skill_weight", label: "End pair skill (8s)", value: cfg.end_pair_skill_weight, min: 0, max: 10, help: "Put skilled rowers at stroke pair + bow pair. They set rhythm and lead." },
                WeightField { name: "engine_room_strength_weight", label: "Engine room (8s)", value: cfg.engine_room_strength_weight, min: 0, max: 10, help: "Put strong rowers in seats 3-6. The power center of the boat." },
                WeightField { name: "pair_eligibility_weight", label: "Pair boat eligibility", value: cfg.pair_eligibility_weight, min: 0, max: 10, help: "Penalize intermediates in pair boats (2-) and mismatched strength. Novices are hard-blocked." },
            ],
        },
        WeightGroup {
            label: "Stacking",
            fields: vec![
                WeightField { name: "top_boat_stacking_weight", label: "Talent ordering", value: cfg.top_boat_stacking_weight, min: -5, max: 10, help: "Positive = rank boats by talent (top boat best, decaying). Negative = spread talent to lower boats." },
                WeightField { name: "boat_size_stacking_weight", label: "Small boat priority", value: cfg.boat_size_stacking_weight, min: 0, max: 10, help: "Stack talent in smaller boats. 4s need strong rowers more than 8s do." },
            ],
        },
        WeightGroup {
            label: "Boat preferences",
            fields: vec![
                WeightField { name: "eight_bias", label: "8+", value: cfg.eight_bias, min: 0, max: 5, help: "Prefer fielding eights over other boats." },
                WeightField { name: "coxed_four_bias", label: "4+", value: cfg.coxed_four_bias, min: 0, max: 5, help: "Prefer fielding coxed fours." },
                WeightField { name: "four_bias", label: "4-", value: cfg.four_bias, min: 0, max: 5, help: "Prefer fielding coxless fours." },
                WeightField { name: "quad_bias", label: "4x", value: cfg.quad_bias, min: 0, max: 5, help: "Prefer fielding quads." },
                WeightField { name: "pair_bias", label: "2-", value: cfg.pair_bias, min: 0, max: 5, help: "Prefer fielding pairs." },
                WeightField { name: "double_bias", label: "2x", value: cfg.double_bias, min: 0, max: 5, help: "Prefer fielding doubles." },
                WeightField { name: "single_bias", label: "1x", value: cfg.single_bias, min: 0, max: 5, help: "Prefer fielding singles." },
            ],
        },
        WeightGroup {
            label: "Other",
            fields: vec![
                WeightField { name: "cox_cooldown_penalty", label: "Cox cooldown", value: cfg.cox_cooldown_penalty, min: 0, max: 10, help: "Avoid coxing the same non-designated rower in consecutive practices." },
                WeightField { name: "bench_cooldown_penalty", label: "Bench cooldown", value: cfg.bench_cooldown_penalty, min: 0, max: 10, help: "Avoid benching the same rower in consecutive practices. Fairer rotation." },
                WeightField { name: "bow_cox_fit_weight", label: "Bow cox fit", value: cfg.bow_cox_fit_weight, min: 0, max: 10, help: "Penalize tall/heavy rowers in bow-loader cox seats. Tight compartment." },
            ],
        },
    ]
}

/// Render the profile editor modal. `name` is the profile name (empty
/// for new). `basis_name` is shown as context ("Based on: Balanced").
/// `is_builtin` means read-only with a "Duplicate" button instead of
/// "Save".
pub(crate) fn profile_editor_modal(
    name: &str,
    basis_name: &str,
    config: &SolverConfig,
    description: Option<&str>,
    is_builtin: bool,
) -> Markup {
    let groups = weight_groups(config);
    let title = if is_builtin {
        format!("Preset: {basis_name}")
    } else if name.is_empty() {
        format!("New preset (based on {basis_name})")
    } else {
        format!("Edit: {name}")
    };

    html! {
        // Backdrop
        div id="profile-modal-backdrop"
            class="fixed inset-0 z-40 modal-backdrop"
            style="background: color-mix(in oklch, var(--ink) 50%, transparent)"
            "@click"="dismissModal('profile-modal', 'profile-modal-backdrop')" {}
        // Modal
        div id="profile-modal"
            role="dialog"
            "aria-modal"="true"
            "aria-label"="Generator profile"
            class="fixed inset-0 z-50 flex items-start justify-center pt-12 px-4 pointer-events-none" {
            div class="rounded-lg shadow-xl w-full max-w-2xl max-h-[80vh] overflow-y-auto pointer-events-auto modal-card"
                style="background: var(--paper); border: 1px solid var(--rule)" {
                // Header
                div class="sticky top-0 px-6 py-4 flex items-center justify-between"
                    style="background: var(--paper); border-bottom: 1px solid var(--rule)" {
                    h2 class="text-lg font-bold font-serif-heading text-ink" {
                        (title)
                    }
                    button type="button"
                           class="text-xl leading-none cursor-pointer text-muted"
                           "aria-label"="Close"
                           "@click"="dismissModal('profile-modal', 'profile-modal-backdrop')" {
                        span "aria-hidden"="true" { "\u{00d7}" }
                    }
                }

                // Form
                form method="post" action="/solver-profile"
                     hx-post="/solver-profile" hx-disabled-elt="find button"
                     hx-target="#profile-modal"
                     hx-swap="delete"
                     class="px-6 py-4 space-y-6" {

                    @if is_builtin {
                        p class="text-sm italic text-muted" {
                            "Built-in presets are read-only. Duplicate to create an editable copy."
                        }
                    }

                    // Name + description
                    div class="grid grid-cols-1 sm:grid-cols-2 gap-4" {
                        div {
                            label for="profile-name" class="block text-xs font-semibold uppercase tracking-wide mb-1 text-muted" {
                                "Name"
                            }
                            input id="profile-name" name="name" type="text"
                                  value=(if is_builtin { "" } else { name })
                                  placeholder=(if is_builtin { format!("Copy of {basis_name}") } else { "My preset".to_string() })
                                  required[!is_builtin]
                                  disabled[false]
                                  class="w-full rounded px-3 py-2 text-sm focus:outline-none"
                                  style="border: 1px solid var(--rule); background: var(--paper-2); color: var(--ink)";
                        }
                        div {
                            label for="profile-desc" class="block text-xs font-semibold uppercase tracking-wide mb-1 text-muted" {
                                "Description"
                            }
                            input id="profile-desc" name="description" type="text"
                                  value=[description]
                                  placeholder="Optional"
                                  class="w-full rounded px-3 py-2 text-sm focus:outline-none"
                                  style="border: 1px solid var(--rule); background: var(--paper-2); color: var(--ink)";
                        }
                    }

                    input type="hidden" name="preset" value=(basis_name);

                    // Weight groups
                    @for group in &groups {
                        div {
                            h3 class="text-xs font-semibold uppercase tracking-wide mb-2 pb-1 font-mono-stat"
                               style="color: var(--muted); border-bottom: 1px solid var(--rule)" {
                                (group.label)
                            }
                            div class="grid grid-cols-1 sm:grid-cols-2 gap-x-6 gap-y-3" {
                                @for field in &group.fields {
                                    (weight_slider(field, is_builtin))
                                }
                            }
                        }
                    }

                    // Actions
                    div class="sticky bottom-0 py-4 flex items-center justify-between"
                        style="background: var(--paper-2); border-top: 1px solid var(--rule)" {
                        @if !is_builtin && !name.is_empty() {
                            button type="button"
                                   class="text-sm font-medium cursor-pointer text-bad"
                                   hx-get={"/confirm?kind=delete-preset&name=" (name)}
                                   hx-target="body"
                                   hx-swap="beforeend" {
                                "Delete"
                            }
                        } @else {
                            span {}
                        }
                        button type="submit"
                               class="btn-accent font-semibold px-6 py-2 shadow transition text-sm" {
                            @if is_builtin { "Duplicate" } @else { "Save" }
                        }
                    }
                }
            }
        }
        script { (maud::PreEscaped("trapFocus(document.getElementById('profile-modal'));")) }
    }
}

fn weight_slider(field: &WeightField, readonly: bool) -> Markup {
    let val_id = format!("{}-val", field.name);
    html! {
        div {
            div class="flex items-center justify-between mb-1" {
                label for=(field.name) class="text-xs font-medium text-ink-2" title=(field.help) {
                    (field.label)
                }
                span id=(&val_id) class="text-xs font-mono-stat w-6 text-right text-accent" {
                    (field.value)
                }
            }
            input name=(field.name) type="range"
                  min=(field.min) max=(field.max)
                  value=(field.value)
                  disabled[readonly]
                  class="range-warm"
                  title=(field.help)
                  oninput={"document.getElementById('" (&val_id) "').textContent = this.value"};
        }
    }
}
