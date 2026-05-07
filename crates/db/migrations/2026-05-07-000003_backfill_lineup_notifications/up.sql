-- Backfill lineup_notification from email_log + committed lineups.
-- For each "lineup" email sent, record all rowers who were in committed
-- lineups for that practice date as notified at the email's sent_at time.
INSERT OR IGNORE INTO lineup_notification (practice_id, rower_id, sent_at)
SELECT DISTINCT
    p.id,
    ls.rower_id,
    el.sent_at
FROM email_log el
JOIN practice p
    ON p.team_id = el.team_id
    AND p.date = el.practice_date
JOIN lineup l
    ON l.practice_id = p.id
    AND l.is_draft = 0
JOIN lineup_seat ls
    ON ls.lineup_id = l.id
WHERE el.email_type = 'lineup';
