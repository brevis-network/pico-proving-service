CREATE TABLE proofs (
    app_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    proof BLOB,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (app_id, task_id),
    FOREIGN KEY (app_id) REFERENCES apps (app_id)
);
