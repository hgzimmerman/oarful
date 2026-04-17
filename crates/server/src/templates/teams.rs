//! Team selector dropdown + management pages.

use std::collections::HashSet;

use lineup_db::boat::types::BoatId;
use lineup_db::boat::Boat;
use lineup_db::rower::types::RowerId;
use lineup_db::rower::Rower;
use lineup_db::team::{PracticeDays, Team, TeamId};
use maud::{html, Markup, PreEscaped};

use super::layout::page_header;

/// Renders a compact team switcher that auto-submits on change.
/// Sits in the navbar's right side. When only one team exists, shows
/// the name as plain text (no dropdown) since there's nothing to
/// switch to.
pub(crate) fn selector(teams: &[Team], active: TeamId, tenant_name: Option<&str>) -> Markup {
    let tenant_prefix = tenant_name.map(|n| {
        html! {
            span class="text-xs text-slate-400 mr-2 hidden 2xl:inline" { (n) " ·" }
        }
    });

    if teams.len() <= 1 {
        let name = teams.first().map(|t| t.name.as_str()).unwrap_or("No team");
        return html! {
            span class="flex items-center" {
                @if let Some(prefix) = tenant_prefix { (prefix) }
                span class="text-sm text-slate-300" { (name) }
            }
        };
    }

    html! {
        form method="post" action="/switch-team"
             class="flex items-center space-x-2" {
            @if let Some(prefix) = tenant_prefix { (prefix) }
            label class="text-xs text-slate-400 uppercase tracking-wide" { "Team" }
            select name="team_id"
                   onchange="this.form.submit()"
                   class="bg-slate-700 text-white text-sm rounded px-2 py-1 border border-slate-600 focus:border-slate-400 focus:outline-none cursor-pointer" {
                @for t in teams {
                    @if t.id == active {
                        option value=(t.id) selected { (t.name) }
                    } @else {
                        option value=(t.id) { (t.name) }
                    }
                }
            }
        }
    }
}

// =====================================================================
// Team management (PD only)
// =====================================================================

pub(crate) fn list_content(teams: &[Team]) -> Markup {
    let subtitle = format!("{} teams", teams.len());
    html! {
        (page_header("Teams", Some(&subtitle)))
        div class="px-4 sm:px-8 py-6 max-w-3xl mx-auto space-y-6" {
            // Create team form
            form method="post" action="/teams"
                 hx-post="/teams"
                 hx-target="#content"
                 hx-push-url="true"
                 class="flex items-end gap-3" {
                div {
                    label for="team_name" class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" {
                        "New team"
                    }
                    input id="team_name" name="name" type="text" required placeholder="Team name"
                          class="border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
                }
                button type="submit"
                       class="bg-slate-800 hover:bg-slate-900 text-white font-semibold px-4 py-2 rounded shadow transition text-sm" {
                    "Create"
                }
            }

            @if teams.is_empty() {
                div class="text-slate-500 italic" { "No teams." }
            } @else {
                div class="bg-white rounded-lg shadow divide-y divide-slate-200" {
                    @for t in teams {
                        a href={"/teams/" (t.id)}
                          hx-get={"/teams/" (t.id)}
                          hx-target="#content"
                          hx-push-url="true"
                          class="flex items-center justify-between px-6 py-4 hover:bg-slate-50 transition cursor-pointer" {
                            div {
                                div class="font-semibold text-slate-800" {
                                    (t.name)
                                    @if t.archived.as_bool() {
                                        span class="ml-2 text-xs font-normal text-red-500" { "(archived)" }
                                    }
                                }
                                div class="text-sm text-slate-500" {
                                    "Self-edit: " (t.self_edit_level)
                                }
                            }
                            span class="text-slate-400" { "→" }
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn detail_content(team: &Team) -> Markup {
    let action = format!("/teams/{}", team.id);
    html! {
        (page_header(&team.name, Some("Team settings")))
        div class="px-4 sm:px-8 py-6 max-w-2xl mx-auto space-y-6" {
            a href="/teams"
              hx-get="/teams"
              hx-target="#content"
              hx-push-url="true"
              class="text-sm text-slate-500 hover:text-slate-800" {
                "← back to teams"
            }

            form method="post" action=(action)
                 hx-post=(action)
                 hx-target="#content"
                 class="bg-white rounded-lg shadow p-6 space-y-4" {
                div {
                    label for="name" class="block text-sm font-semibold text-slate-700 mb-1" {
                        "Team name"
                    }
                    input id="name" name="name" type="text" required
                          value=(team.name)
                          class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
                }
                div {
                    label for="self_edit_level" class="block text-sm font-semibold text-slate-700 mb-1" {
                        "Member self-edit level"
                    }
                    select id="self_edit_level" name="self_edit_level"
                           class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none" {
                        option value="low" selected[team.self_edit_level == "low"] {
                            "Low — side, cox, scull only"
                        }
                        option value="medium" selected[team.self_edit_level == "medium"] {
                            "Medium — + height"
                        }
                        option value="high" selected[team.self_edit_level == "high"] {
                            "High — all attributes (except active)"
                        }
                    }
                    p class="text-xs text-slate-500 mt-1" {
                        "Controls which attributes members can edit on their own profile. Coach+ always has full access."
                    }
                }
                div class="grid grid-cols-1 sm:grid-cols-2 gap-4" {
                    div {
                        label for="default_practice_time" class="block text-sm font-semibold text-slate-700 mb-1" {
                            "Default practice time"
                        }
                        @let time_value = team.default_practice_time.map(|t| t.format("%H:%M").to_string()).unwrap_or_default();
                        input id="default_practice_time" name="default_practice_time" type="time"
                              value=(time_value)
                              class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
                        p class="text-xs text-slate-500 mt-1" {
                            "Pre-fills the time when creating new practices."
                        }
                    }
                    div {
                        label for="default_practice_duration" class="block text-sm font-semibold text-slate-700 mb-1" {
                            "Default duration (minutes)"
                        }
                        @let dur_value = team.default_practice_duration_minutes.map(|m| m.to_string()).unwrap_or_default();
                        input id="default_practice_duration" name="default_practice_duration_minutes" type="number"
                              min="1" step="1"
                              value=(dur_value)
                              placeholder="e.g. 90"
                              class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
                        p class="text-xs text-slate-500 mt-1" {
                            "Used for cross-team overlap detection."
                        }
                    }
                }
                div {
                    label class="flex items-center gap-3 cursor-pointer" {
                        input type="checkbox" name="assume_available" value="1"
                              checked[team.assume_available.as_bool()]
                              class="rounded border-slate-300 text-slate-800 focus:ring-slate-500";
                        div {
                            div class="text-sm font-semibold text-slate-700" {
                                "Assume available by default"
                            }
                            p class="text-xs text-slate-500" {
                                "When on, rowers who haven't responded are included in lineups. When off (default), no response means excluded."
                            }
                        }
                    }
                }
                div {
                    label class="block text-sm font-semibold text-slate-700 mb-2" {
                        "Default practice days"
                    }
                    @let days = team.default_practice_days.unwrap_or(PracticeDays::EMPTY);
                    div class="flex flex-wrap gap-3" {
                        @for (abbr, name, weekday) in &[
                            ("Mon", "day_mon", chrono::Weekday::Mon),
                            ("Tue", "day_tue", chrono::Weekday::Tue),
                            ("Wed", "day_wed", chrono::Weekday::Wed),
                            ("Thu", "day_thu", chrono::Weekday::Thu),
                            ("Fri", "day_fri", chrono::Weekday::Fri),
                            ("Sat", "day_sat", chrono::Weekday::Sat),
                            ("Sun", "day_sun", chrono::Weekday::Sun),
                        ] {
                            label class="flex items-center gap-1.5 text-sm cursor-pointer" {
                                input type="checkbox" name=(name) value="1"
                                      checked[days.contains(*weekday)]
                                      class="rounded border-slate-300 text-slate-800 focus:ring-slate-500";
                                (abbr)
                            }
                        }
                    }
                    p class="text-xs text-slate-500 mt-1" {
                        "Pre-fills the next practice date on the Planning page."
                    }
                }
                button type="submit"
                       class="bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 rounded shadow transition" {
                    "Save"
                }
            }

            // Threshold config section
            (threshold_slider_script())
            (threshold_section(team))

            // Archive / unarchive section
            section class="border-t border-red-200 pt-4" {
                @if team.archived.as_bool() {
                    div class="flex items-center gap-3" {
                        span class="text-sm text-red-600 font-medium" { "This team is archived." }
                        form method="post" action={"/teams/" (team.id) "/toggle-archive"}
                             hx-post={"/teams/" (team.id) "/toggle-archive"}
                             hx-target="#content" {
                            button type="submit"
                                   class="text-sm text-emerald-600 hover:text-emerald-800 font-medium py-2" {
                                "Unarchive"
                            }
                        }
                    }
                } @else {
                    button type="button"
                           hx-get={"/confirm?kind=archive-team&id=" (team.id)}
                           hx-target="body"
                           hx-swap="beforeend"
                           class="text-sm text-red-600 hover:text-red-800 font-medium py-2" {
                        "Archive team"
                    }
                }
            }
        }
    }
}

// =====================================================================
// Threshold config — segmented slider with histogram overlay
// =====================================================================

fn threshold_section(team: &Team) -> Markup {
    let team_id = team.id;
    html! {
        section class="border-t border-slate-200 pt-4 mt-4" {
            h3 class="text-sm font-semibold text-slate-700 mb-3" { "Rower bucketing thresholds" }
            p class="text-xs text-slate-500 mb-4" {
                "Drag the carets to define boundaries between categorical buckets. "
                "Rowers with raw metric values will be auto-bucketed on save."
            }

            (threshold_slider(team_id, "weight", "Weight (lbs)",
                &["Lightweight", "Middleweight", "Heavyweight", "Very heavy"],
                130.0, 230.0, 150.0, 175.0, 200.0))

            (threshold_slider(team_id, "height", "Height (inches)",
                &["Short", "Medium", "Tall", "Very tall"],
                60.0, 80.0, 66.0, 70.0, 74.0))

            @let erg_dist = team.erg_threshold_distance_m.unwrap_or(2000);
            (threshold_slider_with_distance(team_id, erg_dist,
                &["Weak", "Intermediate", "Strong", "Very strong"],
                80.0, 140.0, 120.0, 110.0, 100.0))
        }
    }
}

fn threshold_slider(
    team_id: TeamId,
    metric: &str,
    label: &str,
    bucket_labels: &[&str; 4],
    range_min: f64,
    range_max: f64,
    default_low: f64,
    default_mid: f64,
    default_high: f64,
) -> Markup {
    let save_url = format!("/teams/{team_id}/thresholds");
    let hist_url = format!("/teams/{team_id}/histogram?metric={metric}");
    // For strength, the slider is inverted (lower split = stronger),
    // so bucket labels read right-to-left visually.
    let is_descending = metric == "strength";
    let labels = if is_descending {
        // Reverse for display: left=slow(weak), right=fast(strong)
        format!(
            "['{}','{}','{}','{}']",
            bucket_labels[3], bucket_labels[2], bucket_labels[1], bucket_labels[0]
        )
    } else {
        format!(
            "['{}','{}','{}','{}']",
            bucket_labels[0], bucket_labels[1], bucket_labels[2], bucket_labels[3]
        )
    };

    html! {
        div class="mb-6"
            "x-data"=(PreEscaped(format!(
                "thresholdSlider('{metric}', '{save_url}', '{hist_url}', {range_min}, {range_max}, {default_low}, {default_mid}, {default_high}, {labels}, {is_descending})"
            ))) {
            div class="flex items-center justify-between mb-1" {
                span class="text-xs font-semibold text-slate-700 uppercase tracking-wide" { (label) }
                div class="flex items-center gap-2" {
                    button type="button" "@click"="save()"
                           class="text-xs font-semibold text-emerald-600 hover:text-emerald-800" {
                        "Save"
                    }
                }
            }
            // Slider track with histogram + carets
            div class="relative h-24 bg-slate-100 rounded select-none touch-none"
                "x-ref"="track"
                "@mousedown"="startDrag($event)"
                "@touchstart.passive"="startDrag($event)" {
                // Histogram bars (rendered from fetched data)
                template "x-for"="bar in bars" {
                    div class="absolute bottom-0 bg-slate-300 rounded-t-sm"
                        ":style"="barStyle(bar)" {}
                }
                // Colored zone backgrounds
                div class="absolute inset-0 flex rounded overflow-hidden pointer-events-none" {
                    div class="bg-blue-100/60" ":style"="'width:'+pct(v1)+'%'" {}
                    div class="bg-green-100/60" ":style"="'width:'+(pct(v2)-pct(v1))+'%'" {}
                    div class="bg-yellow-100/60" ":style"="'width:'+(pct(v3)-pct(v2))+'%'" {}
                    div class="bg-red-100/60" ":style"="'width:'+(100-pct(v3))+'%'" {}
                }
                // Caret lines
                template "x-for"="(v, i) in [v1, v2, v3]" {
                    div class="absolute top-0 bottom-0 w-0.5 bg-slate-600 cursor-ew-resize"
                        ":style"="'left:'+pct(v)+'%'"
                        ":data-caret"="i" {}
                }
                // Caret handles (larger touch targets)
                template "x-for"="(v, i) in [v1, v2, v3]" {
                    div class="absolute top-1/2 -translate-y-1/2 w-4 h-8 -ml-2 bg-slate-700 rounded cursor-ew-resize shadow"
                        ":style"="'left:'+pct(v)+'%'"
                        ":data-caret"="i" {}
                }
            }
            // Bucket labels
            div class="flex text-[10px] text-slate-500 mt-1" {
                template "x-for"="(lbl, i) in labels" {
                    div class="text-center truncate" ":style"="zoneStyle(i)" {
                        span "x-text"="lbl" {}
                    }
                }
            }
            // Value readout
            div class="flex gap-4 text-[10px] text-slate-400 mt-0.5" {
                span { "Low/Mid: " span "x-text"="fmt(v1)" {} }
                span { "Mid/High: " span "x-text"="fmt(v2)" {} }
                span { "High/Very: " span "x-text"="fmt(v3)" {} }
            }
            // Result toast
            div "x-ref"="result" {}
        }
    }
}

/// Strength slider variant with an erg distance selector.
fn threshold_slider_with_distance(
    team_id: TeamId,
    erg_dist: i32,
    bucket_labels: &[&str; 4],
    range_min: f64,
    range_max: f64,
    default_low: f64,
    default_mid: f64,
    default_high: f64,
) -> Markup {
    let save_url = format!("/teams/{team_id}/thresholds");
    let hist_url_base = format!("/teams/{team_id}/histogram?metric=strength");
    // Reverse labels for descending metric.
    let labels = format!(
        "['{}','{}','{}','{}']",
        bucket_labels[3], bucket_labels[2], bucket_labels[1], bucket_labels[0]
    );

    html! {
        div class="mb-6"
            "x-data"=(PreEscaped(format!(
                "{{ ...thresholdSlider('strength', '{save_url}', '{hist_url_base}', {range_min}, {range_max}, {default_low}, {default_mid}, {default_high}, {labels}, true), ergDist: {erg_dist} }}"
            )))
            "x-init"=(PreEscaped(format!(
                "fetch('{hist_url_base}&dist=' + ergDist).then(r=>r.json()).then(d=>{{ bars=d }}).catch(()=>{{}})"
            ))) {
            div class="flex items-center justify-between mb-1" {
                div class="flex items-center gap-2" {
                    span class="text-xs font-semibold text-slate-700 uppercase tracking-wide" { "Erg split (sec/500m)" }
                    select "x-model"="ergDist"
                           "@change"=(PreEscaped(format!(
                               "fetch('{hist_url_base}&dist=' + ergDist).then(r=>r.json()).then(d=>{{ bars=d }}).catch(()=>{{}})"
                           )))
                           class="border border-slate-300 rounded px-2 py-0.5 text-xs focus:border-slate-500 focus:outline-none" {
                        option value="1000" selected[erg_dist == 1000] { "1k" }
                        option value="2000" selected[erg_dist == 2000] { "2k" }
                        option value="5000" selected[erg_dist == 5000] { "5k" }
                        option value="6000" selected[erg_dist == 6000] { "6k" }
                    }
                }
                button type="button" "@click"="save()"
                       class="text-xs font-semibold text-emerald-600 hover:text-emerald-800" {
                    "Save"
                }
            }
            // Slider track
            div class="relative h-24 bg-slate-100 rounded select-none touch-none"
                "x-ref"="track"
                "@mousedown"="startDrag($event)"
                "@touchstart.passive"="startDrag($event)" {
                template "x-for"="bar in bars" {
                    div class="absolute bottom-0 bg-slate-300 rounded-t-sm"
                        ":style"="barStyle(bar)" {}
                }
                div class="absolute inset-0 flex rounded overflow-hidden pointer-events-none" {
                    div class="bg-blue-100/60" ":style"="'width:'+pct(v1)+'%'" {}
                    div class="bg-green-100/60" ":style"="'width:'+(pct(v2)-pct(v1))+'%'" {}
                    div class="bg-yellow-100/60" ":style"="'width:'+(pct(v3)-pct(v2))+'%'" {}
                    div class="bg-red-100/60" ":style"="'width:'+(100-pct(v3))+'%'" {}
                }
                template "x-for"="(v, i) in [v1, v2, v3]" {
                    div class="absolute top-0 bottom-0 w-0.5 bg-slate-600 cursor-ew-resize"
                        ":style"="'left:'+pct(v)+'%'"
                        ":data-caret"="i" {}
                }
                template "x-for"="(v, i) in [v1, v2, v3]" {
                    div class="absolute top-1/2 -translate-y-1/2 w-4 h-8 -ml-2 bg-slate-700 rounded cursor-ew-resize shadow"
                        ":style"="'left:'+pct(v)+'%'"
                        ":data-caret"="i" {}
                }
            }
            div class="flex text-[10px] text-slate-500 mt-1" {
                template "x-for"="(lbl, i) in labels" {
                    div class="text-center truncate" ":style"="zoneStyle(i)" {
                        span "x-text"="lbl" {}
                    }
                }
            }
            div class="flex gap-4 text-[10px] text-slate-400 mt-0.5" {
                span { "Strong/Very: " span "x-text"="fmt(v1)" {} }
                span { "Inter/Strong: " span "x-text"="fmt(v2)" {} }
                span { "Weak/Inter: " span "x-text"="fmt(v3)" {} }
            }
            div "x-ref"="result" {}
        }
    }
}

fn threshold_slider_script() -> Markup {
    html! {
        script {
            (PreEscaped(r#"
window.thresholdSlider = function(metric, saveUrl, histUrl, rMin, rMax, dLow, dMid, dHigh, labels, desc) { return {
    v1: dLow, v2: dMid, v3: dHigh,
    rMin, rMax, labels,
    bars: [],
    dragging: null,
    init() {
      fetch(histUrl).then(r=>r.json()).then(d=>{this.bars=d}).catch(()=>{});
    },
    pct(v) { return ((v - this.rMin) / (this.rMax - this.rMin) * 100); },
    valFromPct(p) { return this.rMin + p / 100 * (this.rMax - this.rMin); },
    fmt(v) {
      if (metric === 'strength') {
        var ts = Math.round(v * 100);
        var m = Math.floor(ts / 6000);
        var s = Math.floor((ts % 6000) / 100);
        var cs = ts % 100;
        return m + ':' + String(s).padStart(2,'0') + '.' + String(cs).padStart(2,'0');
      }
      if (metric === 'height') {
        var ft = Math.floor(v / 12);
        var inch = Math.round(v % 12);
        return ft + "'" + inch + '"';
      }
      return v.toFixed(1) + (metric === 'weight' ? ' lbs' : '');
    },
    barStyle(bar) {
      if (!this.bars.length) return 'display:none';
      var maxC = Math.max(...this.bars.map(b=>b.count), 1);
      var left = this.pct(bar.min);
      var w = this.pct(bar.max) - left;
      var h = bar.count / maxC * 80;
      return 'left:'+left+'%;width:'+w+'%;height:'+h+'%;opacity:0.5';
    },
    zoneStyle(i) {
      var pts = [this.rMin, this.v1, this.v2, this.v3, this.rMax];
      var w = this.pct(pts[i+1]) - this.pct(pts[i]);
      return 'width:'+w+'%';
    },
    startDrag(e) {
      var rect = this.$refs.track.getBoundingClientRect();
      var x = (e.touches ? e.touches[0].clientX : e.clientX);
      var pct = (x - rect.left) / rect.width * 100;
      var val = this.valFromPct(pct);
      // Find nearest caret
      var vs = [this.v1, this.v2, this.v3];
      var dists = vs.map(v => Math.abs(v - val));
      this.dragging = dists.indexOf(Math.min(...dists));
      this.onDrag(e);
      var moveHandler = (ev) => this.onDrag(ev);
      var upHandler = () => {
        this.dragging = null;
        window.removeEventListener('mousemove', moveHandler);
        window.removeEventListener('mouseup', upHandler);
        window.removeEventListener('touchmove', moveHandler);
        window.removeEventListener('touchend', upHandler);
      };
      window.addEventListener('mousemove', moveHandler);
      window.addEventListener('mouseup', upHandler);
      window.addEventListener('touchmove', moveHandler, {passive: true});
      window.addEventListener('touchend', upHandler);
    },
    onDrag(e) {
      if (this.dragging === null) return;
      var rect = this.$refs.track.getBoundingClientRect();
      var x = (e.touches ? e.touches[0].clientX : e.clientX);
      var pct = Math.max(0, Math.min(100, (x - rect.left) / rect.width * 100));
      var val = this.valFromPct(pct);
      // Enforce ordering with min gap
      var gap = (this.rMax - this.rMin) * 0.01;
      if (this.dragging === 0) {
        this.v1 = Math.min(val, this.v2 - gap);
        this.v1 = Math.max(this.v1, this.rMin);
      } else if (this.dragging === 1) {
        this.v2 = Math.max(val, this.v1 + gap);
        this.v2 = Math.min(this.v2, this.v3 - gap);
      } else {
        this.v3 = Math.max(val, this.v2 + gap);
        this.v3 = Math.min(this.v3, this.rMax);
      }
    },
    save() {
      // For descending metrics, v1 (leftmost/fastest) = high_very boundary,
      // v3 (rightmost/slowest) = low_mid boundary. Swap for storage.
      var body = desc
        ? {metric: metric, low_mid: this.v3, mid_high: this.v2, high_very: this.v1}
        : {metric: metric, low_mid: this.v1, mid_high: this.v2, high_very: this.v3};
      if (this.ergDist) body.erg_distance_m = parseInt(this.ergDist);
      fetch(saveUrl, {method:'POST', headers:{'Content-Type':'application/json'}, body: JSON.stringify(body)})
        .then(r=>r.text()).then(html=>{this.$refs.result.innerHTML=html; setTimeout(()=>{this.$refs.result.innerHTML=''},3000)})
        .catch(()=>{this.$refs.result.innerHTML='<div class="text-sm text-red-600 mt-2">Save failed.</div>'});
    }
  }; };
"#))
        }
    }
}

// =====================================================================
// Roster assignment matrix — rowers × teams
// =====================================================================

pub(crate) fn roster_matrix(
    rowers: &[Rower],
    teams: &[Team],
    memberships: &HashSet<(TeamId, RowerId)>,
) -> Markup {
    roster_matrix_inner(None, rowers, teams, memberships)
}

pub(crate) fn roster_matrix_with_toast(
    message: &str,
    rowers: &[Rower],
    teams: &[Team],
    memberships: &HashSet<(TeamId, RowerId)>,
) -> Markup {
    roster_matrix_inner(Some(message), rowers, teams, memberships)
}

fn roster_matrix_inner(
    toast: Option<&str>,
    rowers: &[Rower],
    teams: &[Team],
    memberships: &HashSet<(TeamId, RowerId)>,
) -> Markup {
    let subtitle = format!("{} rowers · {} teams", rowers.len(), teams.len());
    html! {
        (page_header("Roster assignments", Some(&subtitle)))
        div class="px-4 sm:px-8 py-6" {
            @if let Some(msg) = toast {
                div class="bg-emerald-50 border border-emerald-200 text-emerald-800 rounded-lg px-6 py-4 text-sm mb-4" {
                    (msg)
                }
            }
            @if teams.is_empty() {
                div class="text-slate-500 italic" { "No teams. Create teams first." }
            } @else if rowers.is_empty() {
                div class="text-slate-500 italic" { "No active rowers." }
            } @else {
                form method="post" action="/admin/roster"
                     hx-post="/admin/roster"
                     hx-target="#admin-tab-content" {
                    div class="flex justify-end mb-3" {
                        button type="submit"
                               class="bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 rounded shadow transition text-sm" {
                            "Save"
                        }
                    }
                    div class="overflow-auto bg-white rounded-lg shadow max-h-[75vh]" {
                        table class="text-xs border-collapse" {
                            thead {
                                tr {
                                    th class="sticky top-0 left-0 z-20 bg-slate-100 px-3 py-2 text-left font-semibold text-slate-700 border-b border-r border-slate-200 min-w-[160px]" {
                                        "Rower"
                                    }
                                    @for team in teams {
                                        th class="sticky top-0 z-10 bg-slate-100 px-3 py-2 text-center font-semibold text-slate-700 border-b border-slate-200 whitespace-nowrap min-w-[80px]" {
                                            (team.name)
                                        }
                                    }
                                }
                            }
                            tbody {
                                @for rower in rowers {
                                    tr class="border-t border-slate-100 hover:bg-slate-50" {
                                        td class="sticky left-0 z-10 bg-white px-3 py-1.5 font-medium text-slate-800 border-r border-slate-200 whitespace-nowrap" {
                                            a href={"/rowers/" (rower.id)}
                                              hx-get={"/rowers/" (rower.id)}
                                              hx-target="#content"
                                              hx-push-url="true"
                                              class="text-blue-700 hover:text-blue-900" {
                                                (rower.name)
                                            }
                                        }
                                        @for team in teams {
                                            td class="text-center border-slate-100 px-1" {
                                                @let field_name = format!("m_{}_{}", team.id, rower.id);
                                                @let checked = memberships.contains(&(team.id, rower.id));
                                                input type="checkbox"
                                                       name=(field_name)
                                                       value="1"
                                                       checked[checked]
                                                       class="w-4 h-4 accent-emerald-600 cursor-pointer";
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div class="flex justify-end mt-3" {
                        button type="submit"
                               class="bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 rounded shadow transition text-sm" {
                            "Save"
                        }
                    }
                }
            }
        }
    }
}

// =====================================================================
// Fleet assignment matrix — boats × teams
// =====================================================================

pub(crate) fn fleet_matrix(
    boats: &[Boat],
    teams: &[Team],
    defaults: &HashSet<(TeamId, BoatId)>,
) -> Markup {
    fleet_matrix_inner(None, boats, teams, defaults)
}

pub(crate) fn fleet_matrix_with_toast(
    message: &str,
    boats: &[Boat],
    teams: &[Team],
    defaults: &HashSet<(TeamId, BoatId)>,
) -> Markup {
    fleet_matrix_inner(Some(message), boats, teams, defaults)
}

fn fleet_matrix_inner(
    toast: Option<&str>,
    boats: &[Boat],
    teams: &[Team],
    defaults: &HashSet<(TeamId, BoatId)>,
) -> Markup {
    let subtitle = format!("{} sweep boats · {} teams", boats.len(), teams.len());
    html! {
        (page_header("Default fleet", Some(&subtitle)))
        div class="px-4 sm:px-8 py-6" {
            @if let Some(msg) = toast {
                div class="bg-emerald-50 border border-emerald-200 text-emerald-800 rounded-lg px-6 py-4 text-sm mb-4" {
                    (msg)
                }
            }
            p class="text-sm text-slate-500 mb-4" {
                "Select which boats are pre-selected in the generation pool for each team. "
                "Single-team tenants default to all boats if none are selected."
            }
            @if teams.is_empty() {
                div class="text-slate-500 italic" { "No teams. Create teams first." }
            } @else if boats.is_empty() {
                div class="text-slate-500 italic" { "No sweep boats in the fleet." }
            } @else {
                form method="post" action="/admin/fleet/defaults"
                     hx-post="/admin/fleet/defaults"
                     hx-target="#admin-fleet-content" {
                    div class="flex justify-end mb-3" {
                        button type="submit"
                               class="bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 rounded shadow transition text-sm" {
                            "Save"
                        }
                    }
                    div class="overflow-auto bg-white rounded-lg shadow max-h-[75vh]" {
                        table class="text-xs border-collapse" {
                            thead {
                                tr {
                                    th class="sticky top-0 left-0 z-20 bg-slate-100 px-3 py-2 text-left font-semibold text-slate-700 border-b border-r border-slate-200 min-w-[160px]" {
                                        "Boat"
                                    }
                                    @for team in teams {
                                        th class="sticky top-0 z-10 bg-slate-100 px-3 py-2 text-center font-semibold text-slate-700 border-b border-slate-200 whitespace-nowrap min-w-[80px]" {
                                            (team.name)
                                        }
                                    }
                                }
                            }
                            tbody {
                                @for boat in boats {
                                    tr class="border-t border-slate-100 hover:bg-slate-50" {
                                        td class="sticky left-0 z-10 bg-white px-3 py-1.5 font-medium text-slate-800 border-r border-slate-200 whitespace-nowrap" {
                                            a href={"/boats/" (boat.id)}
                                              hx-get={"/boats/" (boat.id)}
                                              hx-target="#content"
                                              hx-push-url="true"
                                              class="text-blue-700 hover:text-blue-900" {
                                                (boat.name)
                                            }
                                            span class="text-slate-400 ml-1" {
                                                "(" (boat.seat_count)
                                                @if boat.has_cox.as_bool() { "+" }
                                                ")"
                                            }
                                        }
                                        @for team in teams {
                                            td class="text-center border-slate-100 px-1" {
                                                @let field_name = format!("b_{}_{}", team.id, boat.id);
                                                @let checked = defaults.contains(&(team.id, boat.id));
                                                input type="checkbox"
                                                       name=(field_name)
                                                       value="1"
                                                       checked[checked]
                                                       class="w-4 h-4 accent-emerald-600 cursor-pointer";
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div class="flex justify-end mt-3" {
                        button type="submit"
                               class="bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 rounded shadow transition text-sm" {
                            "Save"
                        }
                    }
                }
            }
        }
    }
}
