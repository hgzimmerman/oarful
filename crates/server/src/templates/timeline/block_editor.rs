//! Editor for bare blocks (Launch/Rest/Turn/Dock).

use lineup_db::timeline::{Block, BlockType};
use maud::{html, Markup};

use super::css;
use super::helpers::{action_buttons, duration_field};

pub(super) fn bare_block_editor(block: &Block, base_url: &str, tl_json: &str, pe: &str) -> Markup {
    let is_structural = block.block_type.is_structural();
    html! {
        div class="mt-3 pt-3" style="border-top: 1px solid var(--rule-2)" {
            div class="flex items-center justify-between mb-3" {
                div class="flex items-center gap-2" {
                    span class="font-mono-stat text-[9px] px-1.5 py-0.5 rounded border font-semibold" style=(css::block_type_css(block.block_type)) { (block.block_type.label()) }
                    @if is_structural {
                        span class="font-mono-stat text-[9px] italic" style="color: var(--muted)" {
                            @if block.block_type == BlockType::Launch { "Fixed start" } @else { "Fixed end — auto-sizes to slack" }
                        }
                    }
                }
                @if !is_structural {
                    (action_buttons(base_url, tl_json, &block.id, pe))
                }
            }
            @if block.block_type != BlockType::Dock {
                form hx-post={(base_url) "/patch-block"} hx-target="#timeline-section" hx-swap="innerHTML" hx-trigger="change" hx-sync="this:replace" {
                    input type="hidden" name="timeline" value=(tl_json);
                    input type="hidden" name="plan_editor" value=(pe);
                    input type="hidden" name="patch_id" value=(block.id);
                    input type="hidden" name="selected" value=(block.id);
                    div class="flex flex-wrap gap-4 items-start" {
                        (duration_field(block.duration.value, block.duration.unit))
                        div {
                            (super::helpers::field_label("Notes"))
                            textarea name="note" rows="1" placeholder="e.g. spin at the dam" class="input-warm text-sm w-full resize-y" { (block.note) }
                        }
                    }
                }
            }
        }
    }
}
