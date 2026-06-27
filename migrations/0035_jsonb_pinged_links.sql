ALTER TABLE user_settings
  ALTER COLUMN pinged_links TYPE jsonb 
    USING COALESCE(pinged_links::jsonb, '[]'::jsonb),
  ALTER COLUMN pinged_links SET DEFAULT '[]'::jsonb,
  ALTER COLUMN pinged_links SET NOT NULL;
