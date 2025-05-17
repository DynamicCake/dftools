CREATE TABLE known_instance (
    id SERIAL PRIMARY KEY,
    public_key bytea NOT NULL UNIQUE,
    domain TEXT NOT NULL UNIQUE
);

-- Yes, this is correct, all instances get set to this instance
-- This is due to me not realizing the entire key thing when storing instances
UPDATE plot
SET instance = null;

ALTER TABLE plot
    ALTER COLUMN instance SET DATA TYPE INTEGER
    USING instance::INTEGER;

ALTER TABLE plot
    ADD CONSTRAINT plot_id_fk
    FOREIGN KEY (instance)
    REFERENCES known_instance(id);
