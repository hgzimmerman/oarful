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
                WeightField { name: "skill_variance_weight", label: "Skill variance", value: cfg.skill_variance_weight, min: 0, max: 10, help: "Penalty for skill spread within a boat" },
                WeightField { name: "pair_affinity_weight", label: "Pair affinity", value: cfg.pair_affinity_weight, min: 0, max: 10, help: "Bonus for seating paired rowers together" },
                WeightField { name: "seat_affinity_weight", label: "Seat affinity", value: cfg.seat_affinity_weight, min: 0, max: 10, help: "Bonus for seating rowers in preferred seats" },
                WeightField { name: "side_preference_weight", label: "Side preference", value: cfg.side_preference_weight, min: 0, max: 10, help: "Penalty for wrong-side placement" },
                WeightField { name: "weight_class_slack_weight", label: "Weight class slack", value: cfg.weight_class_slack_weight, min: 0, max: 10, help: "Penalty for weight class mismatch" },
            ],
        },
        WeightGroup {
            label: "Placement",
            fields: vec![
                WeightField { name: "placement_reward_weight", label: "Placement reward", value: cfg.placement_reward_weight, min: 0, max: 10, help: "Reward for fielding each boat" },
                WeightField { name: "partial_fill_bonus", label: "Partial fill bonus", value: cfg.partial_fill_bonus, min: 0, max: 10, help: "Reward for filling optional seats" },
                WeightField { name: "minimize_bench_weight", label: "Minimize bench", value: cfg.minimize_bench_weight, min: 0, max: 10, help: "Reward for placing every rower" },
                WeightField { name: "non_scull_retention_weight", label: "Non-scull retention", value: cfg.non_scull_retention_weight, min: 0, max: 10, help: "Extra reward for placing sweep-only rowers" },
            ],
        },
        WeightGroup {
            label: "Boat structure",
            fields: vec![
                WeightField { name: "pair_strength_weight", label: "Pair strength", value: cfg.pair_strength_weight, min: 0, max: 10, help: "Penalty for strength mismatch in seat pairs" },
                WeightField { name: "bow_pair_strength_weight", label: "Bow pair strength", value: cfg.bow_pair_strength_weight, min: 0, max: 10, help: "Extra penalty for bow pair mismatch" },
                WeightField { name: "height_balance_weight", label: "Height balance", value: cfg.height_balance_weight, min: 0, max: 10, help: "Penalty for height mismatch in seat pairs" },
                WeightField { name: "end_pair_skill_weight", label: "End pair skill", value: cfg.end_pair_skill_weight, min: 0, max: 10, help: "Reward for skilled rowers in end pairs (8s)" },
                WeightField { name: "engine_room_strength_weight", label: "Engine room strength", value: cfg.engine_room_strength_weight, min: 0, max: 10, help: "Reward for strong rowers in middle seats (8s)" },
                WeightField { name: "pair_eligibility_weight", label: "Pair eligibility", value: cfg.pair_eligibility_weight, min: 0, max: 10, help: "Penalty for intermediates in pairs + strength mismatch" },
            ],
        },
        WeightGroup {
            label: "Stacking",
            fields: vec![
                WeightField { name: "top_boat_stacking_weight", label: "Top boat stacking", value: cfg.top_boat_stacking_weight, min: -5, max: 10, help: "Positive = stack top boat, negative = spread talent" },
                WeightField { name: "boat_size_stacking_weight", label: "Boat-size stacking", value: cfg.boat_size_stacking_weight, min: 0, max: 10, help: "Concentrate talent in smaller boats (for speed parity)" },
            ],
        },
        WeightGroup {
            label: "Boat preferences",
            fields: vec![
                WeightField { name: "eight_bias", label: "8+", value: cfg.eight_bias, min: 0, max: 5, help: "Prefer fielding eights" },
                WeightField { name: "coxed_four_bias", label: "4+", value: cfg.coxed_four_bias, min: 0, max: 5, help: "Prefer fielding coxed fours" },
                WeightField { name: "four_bias", label: "4-", value: cfg.four_bias, min: 0, max: 5, help: "Prefer fielding coxless fours" },
                WeightField { name: "quad_bias", label: "4x", value: cfg.quad_bias, min: 0, max: 5, help: "Prefer fielding quads" },
                WeightField { name: "pair_bias", label: "2-", value: cfg.pair_bias, min: 0, max: 5, help: "Prefer fielding pairs" },
                WeightField { name: "double_bias", label: "2x", value: cfg.double_bias, min: 0, max: 5, help: "Prefer fielding doubles" },
                WeightField { name: "single_bias", label: "1x", value: cfg.single_bias, min: 0, max: 5, help: "Prefer fielding singles" },
            ],
        },
        WeightGroup {
            label: "Other",
            fields: vec![
                WeightField { name: "cox_cooldown_penalty", label: "Cox cooldown", value: cfg.cox_cooldown_penalty, min: 0, max: 10, help: "Penalty for non-designated cox within cooldown" },
                WeightField { name: "bow_cox_fit_weight", label: "Bow cox fit", value: cfg.bow_cox_fit_weight, min: 0, max: 10, help: "Penalty for tall/heavy rowers in bow-loader cox" },
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
            class="fixed inset-0 bg-black/40 z-40"
            "@click"="document.getElementById('profile-modal').remove(); document.getElementById('profile-modal-backdrop').remove()" {}
        // Modal
        div id="profile-modal"
            class="fixed inset-0 z-50 flex items-start justify-center pt-12 px-4 pointer-events-none" {
            div class="bg-white rounded-lg shadow-xl w-full max-w-2xl max-h-[80vh] overflow-y-auto pointer-events-auto" {
                // Header
                div class="sticky top-0 bg-white border-b border-slate-200 px-6 py-4 flex items-center justify-between" {
                    h2 class="text-lg font-bold text-slate-800" { (title) }
                    button type="button"
                           class="text-slate-400 hover:text-slate-600 text-xl leading-none"
                           "@click"="document.getElementById('profile-modal').remove(); document.getElementById('profile-modal-backdrop').remove()" {
                        "\u{00d7}"
                    }
                }

                // Form
                form method="post" action="/solver-profile"
                     hx-post="/solver-profile"
                     hx-target="#content"
                     class="px-6 py-4 space-y-6" {

                    @if is_builtin {
                        p class="text-sm text-slate-500 italic" {
                            "Built-in presets are read-only. Duplicate to create an editable copy."
                        }
                    }

                    // Name + description
                    div class="grid grid-cols-1 sm:grid-cols-2 gap-4" {
                        div {
                            label for="profile-name" class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" {
                                "Name"
                            }
                            input id="profile-name" name="name" type="text"
                                  value=(if is_builtin { "" } else { name })
                                  placeholder=(if is_builtin { format!("Copy of {basis_name}") } else { "My preset".to_string() })
                                  required[!is_builtin]
                                  disabled[false]
                                  class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
                        }
                        div {
                            label for="profile-desc" class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" {
                                "Description"
                            }
                            input id="profile-desc" name="description" type="text"
                                  value=[description]
                                  placeholder="Optional"
                                  class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
                        }
                    }

                    // Hidden field to carry the basis preset name (used by
                    // the save handler to resolve defaults for missing fields).
                    input type="hidden" name="preset" value=(basis_name);

                    // Weight groups
                    @for group in &groups {
                        div {
                            h3 class="text-xs font-semibold text-slate-600 uppercase tracking-wide mb-2 border-b border-slate-100 pb-1" {
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
                    div class="sticky bottom-0 bg-white border-t border-slate-200 py-4 flex items-center justify-between" {
                        @if !is_builtin && !name.is_empty() {
                            button type="button"
                                   class="text-sm text-red-500 hover:text-red-700 font-medium"
                                   hx-delete={"/solver-profile/" (name)}
                                   hx-confirm={"Delete preset \"" (name) "\"?"}
                                   hx-target="#content" {
                                "Delete"
                            }
                        } @else {
                            span {}
                        }
                        button type="submit"
                               class="bg-slate-800 hover:bg-slate-900 text-white font-semibold px-6 py-2 rounded shadow transition text-sm" {
                            @if is_builtin { "Duplicate" } @else { "Save" }
                        }
                    }
                }
            }
        }
    }
}

fn weight_slider(field: &WeightField, readonly: bool) -> Markup {
    let val_id = format!("{}-val", field.name);
    html! {
        div {
            div class="flex items-center justify-between mb-1" {
                label for=(field.name) class="text-xs font-medium text-slate-700" title=(field.help) {
                    (field.label)
                }
                span id=(&val_id) class="text-xs font-mono text-slate-500 w-6 text-right" {
                    (field.value)
                }
            }
            input name=(field.name) type="range"
                  min=(field.min) max=(field.max)
                  value=(field.value)
                  disabled[readonly]
                  class="w-full accent-slate-700 h-1.5"
                  title=(field.help)
                  oninput={"document.getElementById('" (&val_id) "').textContent = this.value"};
        }
    }
}
