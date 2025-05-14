CREATE TABLE baton_trust (
    id SERIAL,
    plot INTEGER NOT NULL REFERENCES plot(id),
    trusted INTEGER NOT NULL REFERENCES plot(id),
    UNIQUE (plot, trusted)
);

