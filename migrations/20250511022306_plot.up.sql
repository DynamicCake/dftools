CREATE TABLE plot (
    id SERIAL PRIMARY KEY NOT NULL, -- DF plot id
    owner_uuid UUID NOT NULL,
    instance TEXT -- NULL means current instance
);

