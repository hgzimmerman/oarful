-- Re-add trial_expires_at column
ALTER TABLE tenant ADD COLUMN trial_expires_at DATETIME;

-- Map Free back to Trial
UPDATE tenant SET billing_status = 'trial'
  WHERE billing_status = 'free';
