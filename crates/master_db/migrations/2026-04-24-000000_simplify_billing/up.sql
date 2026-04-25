-- Simplify billing: Trial/Suspended/Cancelled → Free
UPDATE tenant SET billing_status = 'free'
  WHERE billing_status IN ('trial', 'suspended', 'cancelled');

-- Drop the trial expiry column (SQLite 3.35.0+)
ALTER TABLE tenant DROP COLUMN trial_expires_at;
