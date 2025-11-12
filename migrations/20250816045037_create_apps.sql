CREATE TABLE apps (
    app_id TEXT PRIMARY KEY NOT NULL,
    program BLOB NOT NULL,
    pk BLOB NOT NULL,
    vk BLOB NOT NULL,
    info TEXT
);
