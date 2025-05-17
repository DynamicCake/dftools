ALTER TABLE plot
    DROP CONSTRAINT IF EXISTS plot_id_fk;

ALTER TABLE plot
    ALTER COLUMN instance SET DATA TYPE TEXT
    USING instance::TEXT;

DROP TABLE IF EXISTS known_instance;

-- Note: The UPDATE statement (setting plot.instance to null)
-- is data manipulation and cannot be reliably reversed without
-- a backup of the original data. This .down script focuses on
-- reverting the schema changes (table creation, column type, constraint).

