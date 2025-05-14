CREATE TABLE api_key (
    id SERIAL,
    plot INTEGER NOT NULL REFERENCES plot(id),
    hashed_key BYTEA NOT NULL UNIQUE,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    disabled BOOLEAN NOT NULL DEFAULT false
);
