//! Rower roster list + per-rower detail page with attribute editing.
//!
//! The list view is read-only — each row has a Details link that
//! navigates to the detail page where attributes, seat affinities,
//! and pair affinities are editable via HTMX section swaps.

use lineup_db::rower::{
    types::{Height, RowerWeightClass, Skill, Strength},
    Rower,
};
use lineup_db::team::SelfEditLevel;

/// Controls what the current user can do on this rower's detail page.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DetailPermissions {
    /// Can edit affinities (seat/pair preferences). Coach+ only.
    pub(crate) can_edit_affinities: bool,
    /// Which attribute fields the user can edit. None = all (Coach+).
    /// Some(level) = restricted by trust level (self-edit).
    pub(crate) self_edit_level: Option<SelfEditLevel>,
}

impl DetailPermissions {
    /// Coach+ — full access.
    pub(crate) fn coach() -> Self {
        Self { can_edit_affinities: true, self_edit_level: None }
    }
    /// Member editing own profile.
    pub(crate) fn member(level: SelfEditLevel) -> Self {
        Self { can_edit_affinities: false, self_edit_level: Some(level) }
    }
    /// Whether a specific field is editable.
    pub(crate) fn can_edit(&self, field: &str) -> bool {
        self.can_edit_field(field)
    }
    fn can_edit_field(&self, field: &str) -> bool {
        match self.self_edit_level {
            None => field != "active", // Coach+ can edit everything except active
            Some(level) => match field {
                "side" | "side_strength" | "sweep_bias" | "can_cox" | "is_designated_cox" => true,
                "height" => level.can_edit_height(),
                "weight_class" => level.can_edit_weight_class(),
                "skill" => level.can_edit_skill(),
                "strength" => level.can_edit_strength(),
                _ => false,
            },
        }
    }
}
use maud::{html, Markup};

use super::layout::{empty_state, page_header};
use crate::handlers::rowers::RowerDetail;

pub(crate) fn list_content(rows: &[Rower], _is_coach: bool) -> Markup {
    let subtitle = format!("{} active members", rows.len());
    html! {
        (page_header("Roster", Some(&subtitle)))
        div class="px-4 sm:px-8 py-6" {
            @if rows.is_empty() {
                (empty_state("No members on file. Sync the spreadsheet to populate the roster."))
            } @else {
                // Mobile: compact card list
                div class="md:hidden bg-white rounded-lg shadow divide-y divide-slate-200" {
                    @for r in rows {
                        (mobile_row(r))
                    }
                }
                // Desktop: full table
                div class="hidden md:block bg-white rounded-lg shadow overflow-x-auto max-w-6xl mx-auto" {
                    table class="w-full text-sm" {
                        thead class="bg-slate-100 text-left text-xs uppercase text-slate-600" {
                            tr {
                                th class="px-4 py-2" { "Name" }
                                th class="px-4 py-2" { "Weight" }
                                th class="px-4 py-2" { "Form" }
                                th class="px-4 py-2" { "Strength" }
                                th class="px-4 py-2" { "Side" }
                                th class="px-4 py-2" { "Cox" }
                                th class="px-4 py-2" { "Sweep bias" }
                            }
                        }
                        tbody {
                            @for r in rows {
                                (static_row(r))
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Toast banner + refreshed roster after a batch invite.
pub(crate) fn batch_invite_result(message: &str, rows: &[Rower], is_coach: bool) -> Markup {
    html! {
        div class="bg-emerald-50 border border-emerald-200 text-emerald-800 rounded-lg px-6 py-4 text-sm mx-4 sm:mx-8 mt-4" {
            (message)
        }
        (list_content(rows, is_coach))
    }
}

/// Mobile card for one rower — name and side.
fn mobile_row(r: &Rower) -> Markup {
    html! {
        a href={"/rowers/" (r.id)}
          hx-get={"/rowers/" (r.id)}
          hx-target="#content"
          hx-push-url="true"
          class="flex items-center justify-between px-4 py-3 hover:bg-slate-50 transition" {
            div {
                div class="font-medium text-blue-700" { (r.name) }
                div class="text-xs text-slate-500" { (side_display_label(r)) }
            }
            div class="flex items-center gap-2" {
                span class="text-slate-400" { "→" }
            }
        }
    }
}

/// Read-only `<tr>` for one rower with a Details link to the full
/// detail/edit page.
fn static_row(r: &Rower) -> Markup {
    html! {
        tr class="border-t border-slate-100 hover:bg-slate-50" {
            td class="px-4 py-2 font-medium" {
                a href={"/rowers/" (r.id)}
                  hx-get={"/rowers/" (r.id)}
                  hx-target="#content"
                  hx-push-url="true"
                  class="text-blue-700 hover:text-blue-900 underline" {
                    (r.name)
                }
            }
            td class="px-4 py-2" { (r.weight_class) }
            td class="px-4 py-2" { (r.skill) }
            td class="px-4 py-2" { (r.strength) }
            td class="px-4 py-2" { (side_display_label(r)) }
            td class="px-4 py-2" {
                @if r.is_designated_cox.as_bool() { "designated" }
                @else if r.can_cox.as_bool() { "yes" }
                @else { "—" }
            }
            td class="px-4 py-2" {
                (r.sweep_bias)
            }
        }
    }
}

// =====================================================================
// Per-rower detail page + affinity sections
// =====================================================================

/// `GET /rowers/{id}` page body. Composed of an attribute summary, a
/// seat-affinities section, and a pair-affinities section. The two
/// affinity sections are also exposed as standalone partials so the
/// CRUD handlers can return just the affected `<section>` for HTMX
/// `outerHTML` swaps.
/// Render the rower detail page. `can_edit_affinities` controls
/// whether the affinity add/delete forms are shown (Coach+ only).
/// Attribute editing is always available for the rower's own profile
/// and for Coach+.
pub(crate) fn detail_content(detail: &RowerDetail, perms: DetailPermissions) -> Markup {
    let r = &detail.rower;
    let subtitle = format!(
        "{} · {} · {} · {}",
        r.weight_class, r.skill, r.strength, r.side,
    );
    html! {
        header class="bg-white border-b border-slate-200 px-4 sm:px-8 py-4 sm:py-6" {
            div class="flex items-center gap-3" {
                a href="/team/roster"
                  onclick="if (history.length > 1) { history.back(); return false; }"
                  class="text-slate-400 hover:text-slate-700"
                  title="Back" {
                    "←"
                }
                h1 class="text-2xl font-bold text-slate-800" { (r.name) }
            }
            p class="text-sm text-slate-500 mt-1" { (subtitle) }
        }
        div class="px-8 py-6 max-w-4xl mx-auto space-y-6" {
            (attribute_section(r, None, &perms))
            (seat_affinities_section(detail, None, perms.can_edit_affinities))
            (pair_affinities_section(detail, None, perms.can_edit_affinities))
        }
    }
}

/// Read-only attribute display with an Edit button that swaps to an
/// inline form. The section has id `#attributes` for HTMX `outerHTML`.
pub(crate) fn attribute_section(r: &Rower, error: Option<&str>, _perms: &DetailPermissions) -> Markup {
    let edit_url = format!("/rowers/{}/edit-attributes", r.id);
    html! {
        section #attributes class="bg-white rounded-lg shadow p-6" {
            div class="flex items-start justify-between mb-4" {
                h2 class="text-lg font-bold text-slate-800" { "Attributes" }
                button type="button"
                       class="text-sm text-slate-500 hover:text-slate-800 font-semibold uppercase tracking-wide"
                       hx-get=(edit_url)
                       hx-target="#attributes"
                       hx-swap="outerHTML" {
                    "Edit"
                }
            }
            @if let Some(msg) = error {
                div class="mb-3 text-xs text-red-700 bg-red-50 border-l-4 border-red-500 px-3 py-2 rounded" {
                    (msg)
                }
            }
            dl class="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-4 gap-3 text-sm" {
                (kv("Weight", &r.weight_class.to_string()))
                (kv("Form", &r.skill.to_string()))
                (kv("Strength", &r.strength.to_string()))
                (kv("Height", &r.height.to_string()))
                (kv("Side", &side_display_label(r)))
                (kv("Can cox", if r.can_cox.as_bool() { "yes" } else { "—" }))
                (kv("Designated", if r.is_designated_cox.as_bool() { "yes" } else { "—" }))
                (kv("Sweep bias", &r.sweep_bias.to_string()))
                (kv("Active", if r.active.as_bool() { "yes" } else { "no" }))
            }
        }
    }
}

/// Editable attribute form. Save posts to `/rowers/{id}` and the
/// handler returns a fresh `attribute_section` for the HTMX swap.
pub(crate) fn attribute_edit_section(r: &Rower, error: Option<&str>, perms: &DetailPermissions) -> Markup {
    let post_url = format!("/rowers/{}", r.id);
    let cancel_url = format!("/rowers/{}/attributes", r.id);
    html! {
        section #attributes class="bg-white rounded-lg shadow p-6 bg-amber-50/50" {
            div class="flex items-start justify-between mb-4" {
                h2 class="text-lg font-bold text-slate-800" { "Edit attributes" }
                div class="flex items-center gap-2" {
                    button type="button"
                           class="bg-emerald-600 hover:bg-emerald-700 text-white text-sm font-semibold px-3 py-1.5 rounded"
                           hx-post=(post_url)
                           hx-include="#attributes"
                           hx-target="#attributes"
                           hx-swap="outerHTML" {
                        "Save"
                    }
                    button type="button"
                           class="text-slate-500 hover:text-slate-800 text-sm font-semibold"
                           hx-get=(cancel_url)
                           hx-target="#attributes"
                           hx-swap="outerHTML" {
                        "Cancel"
                    }
                }
            }
            @if let Some(msg) = error {
                div class="mb-3 text-xs text-red-700 bg-red-50 border-l-4 border-red-500 px-3 py-2 rounded" {
                    (msg)
                }
            }
            div class="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-4 gap-3 text-sm" {
                // Weight class
                @if perms.can_edit_field("weight_class") {
                    div {
                        label class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Weight" }
                        (enum_select("weight_class", &[
                            ("Light", "Lightweight", RowerWeightClass::Light == r.weight_class),
                            ("Medium", "Middleweight", RowerWeightClass::Medium == r.weight_class),
                            ("Heavy", "Heavyweight", RowerWeightClass::Heavy == r.weight_class),
                        ]))
                    }
                } @else {
                    (kv("Weight", &r.weight_class.to_string()))
                    input type="hidden" name="weight_class" value=(match r.weight_class {
                        RowerWeightClass::Light => "Light",
                        RowerWeightClass::Medium => "Medium",
                        RowerWeightClass::Heavy => "Heavy",
                    });
                }
                // Form (skill)
                @if perms.can_edit_field("skill") {
                    div {
                        label class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Form" }
                        (enum_select("skill", &[
                            ("Novice", "Novice", Skill::Novice == r.skill),
                            ("Intermediate", "Intermediate", Skill::Intermediate == r.skill),
                            ("Master", "Master", Skill::Master == r.skill),
                            ("Expert", "Expert", Skill::Expert == r.skill),
                        ]))
                    }
                } @else {
                    (kv("Form", &r.skill.to_string()))
                    input type="hidden" name="skill" value=(match r.skill {
                        Skill::Novice => "Novice",
                        Skill::Intermediate => "Intermediate",
                        Skill::Master => "Master",
                        Skill::Expert => "Expert",
                    });
                }
                // Strength
                @if perms.can_edit_field("strength") {
                    div {
                        label class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Strength" }
                        (enum_select("strength", &[
                            ("Weak", "Weak", Strength::Weak == r.strength),
                            ("Intermediate", "Intermediate", Strength::Intermediate == r.strength),
                            ("Strong", "Strong", Strength::Strong == r.strength),
                            ("VeryStrong", "Very strong", Strength::VeryStrong == r.strength),
                        ]))
                    }
                } @else {
                    (kv("Strength", &r.strength.to_string()))
                    input type="hidden" name="strength" value=(match r.strength {
                        Strength::Weak => "Weak",
                        Strength::Intermediate => "Intermediate",
                        Strength::Strong => "Strong",
                        Strength::VeryStrong => "VeryStrong",
                    });
                }
                // Height
                @if perms.can_edit_field("height") {
                    div {
                        label class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Height" }
                        (enum_select("height", &[
                            ("Short", "Short", Height::Short == r.height),
                            ("Medium", "Medium", Height::Medium == r.height),
                        ("Tall", "Tall", Height::Tall == r.height),
                        ("VeryTall", "Very tall", Height::VeryTall == r.height),
                    ]))
                    }
                } @else {
                    (kv("Height", &r.height.to_string()))
                    input type="hidden" name="height" value=(match r.height {
                        Height::Short => "Short",
                        Height::Medium => "Medium",
                        Height::Tall => "Tall",
                        Height::VeryTall => "VeryTall",
                    });
                }
                // Side — always editable
                div class="col-span-2" {
                    (side_slider(r))
                }
                div {
                    label class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Cox" }
                    (checkbox("can_cox", "can cox", r.can_cox.as_bool()))
                    (checkbox("is_designated_cox", "designated", r.is_designated_cox.as_bool()))
                }
                div {
                    label class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Sweep bias" }
                    select name="sweep_bias"
                           class="border border-slate-300 rounded px-2 py-1 text-xs focus:border-slate-500 focus:outline-none" {
                        @for val in [-2, -1, 0, 1, 2].iter() {
                            @let label_text = match val {
                                -2 => "Scull only (-2)",
                                -1 => "Prefers scull (-1)",
                                0 => "No preference (0)",
                                1 => "Prefers sweep (1)",
                                2 => "Sweep only (2)",
                                _ => "",
                            };
                            @if *val == r.sweep_bias.as_int() {
                                option value=(val) selected { (label_text) }
                            } @else {
                                option value=(val) { (label_text) }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn kv(label: &str, value: &str) -> Markup {
    html! {
        div class="bg-slate-50 rounded p-2" {
            div class="text-xs text-slate-500 uppercase tracking-wide" { (label) }
            div class="font-medium text-slate-800" { (value) }
        }
    }
}

/// Seat-affinities section. Standalone so the CRUD handlers can return
/// it as their HTMX response (`outerHTML` swap on `#seat-affinities`).
pub(crate) fn seat_affinities_section(
    detail: &RowerDetail,
    error: Option<&str>,
    can_edit: bool,
) -> Markup {
    let r = &detail.rower;
    let upsert_url = format!("/rowers/{}/seat-affinity", r.id);
    let delete_url = format!("/rowers/{}/seat-affinity/delete", r.id);
    html! {
        section #seat-affinities class="bg-white rounded-lg shadow p-6" {
            div class="flex items-center justify-between mb-3" {
                h2 class="text-lg font-bold text-slate-800" { "Seat preferences" }
                span class="text-xs text-slate-500" {
                    "Per-seat reward / penalty (S3)"
                }
            }
            @if let Some(msg) = error {
                div class="mb-3 text-xs text-red-700 bg-red-50 border-l-4 border-red-500 px-3 py-2 rounded" {
                    (msg)
                }
            }
            @if detail.seat_affinities.is_empty() {
                div class="text-sm text-slate-500 italic mb-3" { "No seat preferences on file." }
            } @else {
                table class="w-full text-sm mb-3" {
                    thead class="text-left text-xs uppercase text-slate-500" {
                        tr {
                            th class="py-1 w-24" { "Seat" }
                            th class="py-1" { "Preference" }
                            th class="py-1" { "" }
                        }
                    }
                    tbody {
                        @for aff in &detail.seat_affinities {
                            tr class="border-t border-slate-100" {
                                td class="py-1" { (aff.zone.display_name()) }
                                td class="py-1 text-sm" { (format_weight(aff.weight.as_int())) }
                                @if can_edit {
                                    td class="py-1 text-right" {
                                        button type="button"
                                               class="text-xs text-red-600 hover:text-red-800"
                                               hx-post=(delete_url)
                                               hx-vals={"{\"zone\": \"" (aff.zone.as_str()) "\"}"}
                                               hx-target="#seat-affinities"
                                               hx-swap="outerHTML" {
                                            "Delete"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            @if !can_edit && detail.seat_affinities.is_empty() {
                div class="text-sm text-slate-500 italic" { "No seat preferences set by your coach." }
            }

            // Add form — only for coaches
            @if can_edit {
            form hx-post=(upsert_url)
                 hx-target="#seat-affinities"
                 hx-swap="outerHTML"
                 class="flex flex-col sm:flex-row sm:items-end gap-2 pt-3 border-t border-slate-200" {
                div {
                    label for="zone" class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Zone" }
                    select id="zone" name="zone"
                           class="border border-slate-300 rounded px-3 py-2 text-sm" {
                        @for z in lineup_db::seat_affinity::SeatZone::ALL {
                            option value=(z.as_str()) { (z.display_name()) }
                        }
                    }
                }
                (weight_slider("seat_weight", 3))
                button type="submit"
                       class="bg-emerald-600 hover:bg-emerald-700 text-white text-sm font-semibold px-4 py-2 rounded" {
                    "Add / update"
                }
            }
            } // @if can_edit
        }
    }
}

/// Pair-affinities section. Standalone so the CRUD handlers can swap
/// it via `outerHTML` on `#pair-affinities`.
pub(crate) fn pair_affinities_section(
    detail: &RowerDetail,
    error: Option<&str>,
    can_edit: bool,
) -> Markup {
    let r = &detail.rower;
    let upsert_url = format!("/rowers/{}/pair-affinity", r.id);
    let delete_url = format!("/rowers/{}/pair-affinity/delete", r.id);
    let lookup = |id: lineup_db::rower::types::RowerId| -> &str {
        if id == r.id {
            r.name.as_str()
        } else {
            detail
                .other_rowers
                .iter()
                .find(|o| o.id == id)
                .map(|o| o.name.as_str())
                .unwrap_or("<unknown>")
        }
    };
    html! {
        section #pair-affinities class="bg-white rounded-lg shadow p-6" {
            div class="flex items-center justify-between mb-3" {
                h2 class="text-lg font-bold text-slate-800" { "Pair preferences" }
                span class="text-xs text-slate-500" {
                    "Same-partition reward / penalty (S2)"
                }
            }
            @if let Some(msg) = error {
                div class="mb-3 text-xs text-red-700 bg-red-50 border-l-4 border-red-500 px-3 py-2 rounded" {
                    (msg)
                }
            }
            @if detail.pair_affinities.is_empty() {
                div class="text-sm text-slate-500 italic mb-3" { "No pair preferences on file." }
            } @else {
                table class="w-full text-sm mb-3" {
                    thead class="text-left text-xs uppercase text-slate-500" {
                        tr {
                            th class="py-1" { "Partner" }
                            th class="py-1" { "Preference" }
                            th class="py-1" { "" }
                        }
                    }
                    tbody {
                        @for aff in &detail.pair_affinities {
                            @let partner_id = if aff.rower_a_id == r.id { aff.rower_b_id } else { aff.rower_a_id };
                            tr class="border-t border-slate-100" {
                                td class="py-1" { (lookup(partner_id)) }
                                td class="py-1 text-sm" { (format_weight(aff.weight.as_int())) }
                                @if can_edit {
                                    td class="py-1 text-right" {
                                        button type="button"
                                               class="text-xs text-red-600 hover:text-red-800"
                                               hx-post=(delete_url)
                                               hx-vals={"{\"partner_id\": " (partner_id) "}"}
                                               hx-target="#pair-affinities"
                                               hx-swap="outerHTML" {
                                            "Delete"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            @if !can_edit && detail.pair_affinities.is_empty() {
                div class="text-sm text-slate-500 italic" { "No pair preferences set by your coach." }
            }

            @if can_edit {
            form hx-post=(upsert_url)
                 hx-target="#pair-affinities"
                 hx-swap="outerHTML"
                 class="flex flex-col sm:flex-row sm:items-end gap-2 pt-3 border-t border-slate-200" {
                div class="flex-grow" {
                    label for="partner_id" class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Partner" }
                    select id="partner_id" name="partner_id"
                           class="w-full border border-slate-300 rounded px-3 py-2 text-sm" {
                        @for o in &detail.other_rowers {
                            option value=(o.id) { (o.name) }
                        }
                    }
                }
                (weight_slider("pair_weight", 3))
                button type="submit"
                       class="bg-emerald-600 hover:bg-emerald-700 text-white text-sm font-semibold px-4 py-2 rounded" {
                    "Add / update"
                }
            }
            } // @if can_edit
        }
    }
}

/// Render a `<select>` with `(db_value, display_label, is_selected)` options.
fn enum_select(name: &str, options: &[(&str, &str, bool)]) -> Markup {
    html! {
        select name=(name)
               class="border border-slate-300 rounded px-2 py-1 text-xs focus:border-slate-500 focus:outline-none" {
            @for (value, label, selected) in options {
                @if *selected {
                    option value=(value) selected { (label) }
                } @else {
                    option value=(value) { (label) }
                }
            }
        }
    }
}

/// Range slider for affinity weights. The UI presents a 1–10 scale
/// (Strongly avoid → Strongly prefer) which maps to the solver's
/// -5..+5 range with 0 skipped. The hidden form input sends the
/// mapped solver value.
fn weight_slider(id: &str, default: i32) -> Markup {
    // Solver value (-5..+5, no 0) → slider position (1..10).
    let slider_pos = if default > 0 { default + 5 } else { default + 6 };
    let default_label = weight_label(slider_pos);

    // JS: map slider position (1..10) → solver value, update label.
    let js = format!(
        "function {id}_update(pos) {{ \
            var sv = pos <= 5 ? pos - 6 : pos - 5; \
            document.getElementById('{id}-hidden').value = sv; \
            var labels = {{ \
                1:'Strongly avoid',2:'Avoid',3:'Moderately avoid', \
                4:'Slightly avoid',5:'Weakly avoid', \
                6:'Weakly prefer',7:'Slightly prefer',8:'Moderately prefer', \
                9:'Prefer',10:'Strongly prefer' \
            }}; \
            document.getElementById('{id}-label').textContent = labels[pos] || pos; \
        }}"
    );

    html! {
        div class="flex-1 min-w-[12rem]" {
            div class="flex items-center justify-between mb-1" {
                span class="text-xs text-red-600 font-semibold" { "Avoid" }
                span #(format!("{id}-label")) class="text-xs font-semibold text-slate-700" {
                    (default_label)
                }
                span class="text-xs text-emerald-600 font-semibold" { "Prefer" }
            }
            input type="range" min="1" max="10" value=(slider_pos)
                  class="w-full accent-blue-600"
                  oninput={(format!("{id}_update(Number(this.value))"))};
            input #(format!("{id}-hidden")) type="hidden" name="weight" value=(default);
            script { (maud::PreEscaped(&js)) }
        }
    }
}

/// Format a solver weight value (-5..+5) as a human-readable label
/// with the numeric value in parentheses, e.g. "Moderately prefer (+3)".
/// Combined side + side_strength as a single slider.
/// -5 = hard port, 0 = either, +5 = hard starboard.
fn side_slider(r: &Rower) -> Markup {
    use lineup_db::rower::types::Side;
    // Map current state to slider position (-5..+5).
    let pos: i32 = match r.side {
        Side::Either => 0,
        Side::Port => {
            let s = r.side_strength.as_int();
            if s == 0 { -5 } else { -(6 - s).min(5).max(1) }
        }
        Side::Starboard => {
            let s = r.side_strength.as_int();
            if s == 0 { 5 } else { (6 - s).min(5).max(1) }
        }
    };
    let default_label = side_slider_label(pos);

    html! {
        div {
            div class="flex items-center justify-between mb-1" {
                span class="text-xs text-red-600 font-semibold" { "Port" }
                span #side-slider-label class="text-xs font-semibold text-slate-700" {
                    (default_label)
                }
                span class="text-xs text-green-600 font-semibold" { "Starboard" }
            }
            input #side-slider type="range" min="-5" max="5" value=(pos)
                  class="w-full accent-blue-600"
                  oninput="sideSliderUpdate(Number(this.value))";
            input #side-hidden type="hidden" name="side" value=(match r.side {
                Side::Port => "Port",
                Side::Starboard => "Starboard",
                Side::Either => "Either",
            });
            input #side-strength-hidden type="hidden" name="side_strength" value=(r.side_strength.as_int());
            script {
                (maud::PreEscaped(r#"
function sideSliderUpdate(v) {
    var labels = {
        '-5':'Hard port','-4':'Strong port','-3':'Moderate port',
        '-2':'Slight port','-1':'Weak port',
        '0':'Either',
        '1':'Weak starboard','2':'Slight starboard','3':'Moderate starboard',
        '4':'Strong starboard','5':'Hard starboard'
    };
    document.getElementById('side-slider-label').textContent = labels[String(v)] || v;
    if (v === 0) {
        document.getElementById('side-hidden').value = 'Either';
        document.getElementById('side-strength-hidden').value = '0';
    } else if (v < 0) {
        document.getElementById('side-hidden').value = 'Port';
        var strength = Math.abs(v) === 5 ? 0 : 6 - Math.abs(v);
        document.getElementById('side-strength-hidden').value = String(strength);
    } else {
        document.getElementById('side-hidden').value = 'Starboard';
        var strength = v === 5 ? 0 : 6 - v;
        document.getElementById('side-strength-hidden').value = String(strength);
    }
}
"#))
            }
        }
    }
}

/// Human-readable side label with numeric value, e.g. "Strong port (-4)".
fn side_display_label(r: &Rower) -> String {
    use lineup_db::rower::types::Side;
    let pos: i32 = match r.side {
        Side::Either => 0,
        Side::Port => {
            let s = r.side_strength.as_int();
            if s == 0 { -5 } else { -(6 - s).min(5).max(1) }
        }
        Side::Starboard => {
            let s = r.side_strength.as_int();
            if s == 0 { 5 } else { (6 - s).min(5).max(1) }
        }
    };
    let label = side_slider_label(pos);
    if pos == 0 {
        label.to_string()
    } else {
        let sign = if pos > 0 { "+" } else { "" };
        format!("{label} ({sign}{pos})")
    }
}

fn side_slider_label(pos: i32) -> &'static str {
    match pos {
        -5 => "Hard port",
        -4 => "Strong port",
        -3 => "Moderate port",
        -2 => "Slight port",
        -1 => "Weak port",
        0 => "Either",
        1 => "Weak starboard",
        2 => "Slight starboard",
        3 => "Moderate starboard",
        4 => "Strong starboard",
        5 => "Hard starboard",
        _ => "?",
    }
}

fn format_weight(w: i32) -> String {
    let label = match w {
        -5 => "Strongly avoid",
        -4 => "Avoid",
        -3 => "Moderately avoid",
        -2 => "Slightly avoid",
        -1 => "Weakly avoid",
        1 => "Weakly prefer",
        2 => "Slightly prefer",
        3 => "Moderately prefer",
        4 => "Prefer",
        5 => "Strongly prefer",
        _ => "?",
    };
    let sign = if w > 0 { "+" } else { "" };
    format!("{label} ({sign}{w})")
}

fn weight_label(slider_pos: i32) -> &'static str {
    match slider_pos {
        1 => "Strongly avoid",
        2 => "Avoid",
        3 => "Moderately avoid",
        4 => "Slightly avoid",
        5 => "Weakly avoid",
        6 => "Weakly prefer",
        7 => "Slightly prefer",
        8 => "Moderately prefer",
        9 => "Prefer",
        10 => "Strongly prefer",
        _ => "?",
    }
}

fn checkbox(name: &str, label: &str, checked: bool) -> Markup {
    html! {
        label class="flex items-center space-x-1 text-xs text-slate-700" {
            @if checked {
                input type="checkbox" name=(name) checked;
            } @else {
                input type="checkbox" name=(name);
            }
            span { (label) }
        }
    }
}
