//! HTML email templates for availability reminders and lineup
//! notifications. These are standalone HTML documents (not wrapped
//! in the app's layout) suitable for email delivery.

use chrono::NaiveDate;
use maud::{html, Markup, DOCTYPE};

use crate::mailer::EmailLineupSummary;

/// Inline styles shared across email templates. Email clients don't
/// support external stylesheets so everything must be inlined.
fn email_wrapper(subject: &str, body: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (subject) }
                style {
                    "body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; \
                     margin: 0; padding: 0; background: #f1f5f9; color: #1e293b; } \
                     .container { max-width: 600px; margin: 0 auto; padding: 24px 16px; } \
                     .card { background: #ffffff; border-radius: 8px; padding: 24px; margin-bottom: 16px; } \
                     .header { font-size: 20px; font-weight: 700; margin-bottom: 16px; } \
                     .subheader { font-size: 14px; font-weight: 600; color: #475569; text-transform: uppercase; \
                      letter-spacing: 0.05em; margin-bottom: 8px; } \
                     .date-item { padding: 8px 0; border-bottom: 1px solid #e2e8f0; font-size: 14px; } \
                     .date-item:last-child { border-bottom: none; } \
                     .boat-card { background: #f8fafc; border-radius: 6px; padding: 12px 16px; margin-bottom: 12px; } \
                     .boat-name { font-weight: 600; font-size: 15px; margin-bottom: 8px; } \
                     .seat-row { display: flex; justify-content: space-between; padding: 3px 0; font-size: 13px; } \
                     .seat-label { color: #64748b; min-width: 60px; } \
                     .rower-name { font-weight: 500; } \
                     .benched { font-size: 13px; color: #64748b; font-style: italic; } \
                     .btn { display: inline-block; background: #1e293b; color: #ffffff !important; \
                      text-decoration: none; padding: 10px 24px; border-radius: 6px; font-weight: 600; \
                      font-size: 14px; } \
                     .footer { text-align: center; font-size: 12px; color: #94a3b8; margin-top: 24px; } \
                     a { color: #1e293b; }"
                }
            }
            body {
                div class="container" {
                    (body)
                    div class="footer" {
                        p { "Sent by Lineup Generator" }
                    }
                }
            }
        }
    }
}

/// Magic-link login email. When there's one club, a single "Sign in"
/// button. When multiple, one button per club so the user picks.
pub(crate) fn magic_login_email(
    to_name: &str,
    clubs: &[(String, String)], // (club_name, magic_url)
) -> Markup {
    let subject = "Sign in to Lineup Generator".to_string();
    email_wrapper(&subject, html! {
        div class="card" {
            div class="header" { "Sign in" }
            p style="font-size: 14px; margin-bottom: 16px;" {
                "Hi " (to_name) ", click below to sign in:"
            }
            @if clubs.len() == 1 {
                div style="text-align: center;" {
                    a href=(&clubs[0].1) class="btn" {
                        "Sign in to " (&clubs[0].0)
                    }
                }
            } @else {
                p style="font-size: 13px; color: #64748b; margin-bottom: 12px;" {
                    "Your email is associated with multiple clubs. Choose one:"
                }
                @for (name, url) in clubs {
                    div style="margin-bottom: 8px;" {
                        a href=(url) class="btn" style="display: block; text-align: center;" {
                            "Sign in to " (name)
                        }
                    }
                }
            }
            p style="font-size: 12px; color: #94a3b8; margin-top: 16px;" {
                "This link expires in 24 hours."
            }
        }
    })
}

/// Availability reminder email: lists practice dates without a response
/// and includes a magic link to the availability page.
pub(crate) fn reminder_email(
    to_name: &str,
    team_name: &str,
    dates: &[NaiveDate],
    magic_url: &str,
) -> Markup {
    let subject = format!("{team_name} — availability needed");
    email_wrapper(&subject, html! {
        div class="card" {
            div class="header" { (subject) }
            p style="font-size: 14px; margin-bottom: 16px;" {
                "Hi " (to_name) ", your coach needs your availability for the following dates:"
            }
            div {
                @for date in dates {
                    div class="date-item" {
                        strong { (date.format("%A")) }
                        " — "
                        (date)
                    }
                }
            }
            div style="margin-top: 20px; text-align: center;" {
                a href=(magic_url) class="btn" {
                    "Update availability"
                }
            }
        }
    })
}

/// Lineup notification email: shows full seat assignments per boat
/// per date, with a magic link to the history page.
pub(crate) fn lineup_email(
    to_name: &str,
    team_name: &str,
    lineups: &[EmailLineupSummary],
    magic_url: &str,
) -> Markup {
    let dates: Vec<String> = lineups.iter().map(|l| l.date.to_string()).collect();
    let subject = format!("{team_name} — lineups posted for {}", dates.join(", "));
    email_wrapper(&subject, html! {
        div class="card" {
            div class="header" { (team_name) " — lineups posted" }
            p style="font-size: 14px; margin-bottom: 16px;" {
                "Hi " (to_name) ", lineups have been posted:"
            }
            @for summary in lineups {
                div style="margin-bottom: 20px;" {
                    div class="subheader" {
                        (summary.date.format("%A")) " — " (summary.date)
                    }
                    @for boat in &summary.boats {
                        div class="boat-card" {
                            div class="boat-name" { (boat.boat_name) }
                            @for seat in &boat.seats {
                                div class="seat-row" {
                                    span class="seat-label" { (seat.label) }
                                    span class="rower-name" { (seat.rower_name) }
                                }
                            }
                        }
                    }
                    @if !summary.benched.is_empty() {
                        div class="benched" {
                            "Bench: " (summary.benched.join(", "))
                        }
                    }
                }
            }
            div style="margin-top: 20px; text-align: center;" {
                a href=(magic_url) class="btn" {
                    "View lineups"
                }
            }
        }
    })
}
