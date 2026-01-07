CREATE TABLE proving_requests (
    app_id TEXT NOT NULL,
    task_id TEXT NOT NULL,

    inputs BLOB,

    -- complete request is deleted when complete, so this table only have pending requests
    status TEXT DEFAULT 'pending' CHECK (status IN ('pending', 'complete', 'failed')),

    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,

    PRIMARY KEY (app_id, task_id),
    FOREIGN KEY (app_id) REFERENCES apps (app_id) ON DELETE CASCADE
);

CREATE INDEX idx_pending_proving_requests_fifo ON proving_requests(created_at) WHERE status = 'pending';

-- trigger for deleting complete requests
CREATE TRIGGER trg_auto_cleanup_complete_proving_requests
AFTER UPDATE ON proving_requests
FOR EACH ROW
WHEN NEW.status = 'complete'
BEGIN
    DELETE FROM proving_requests
    WHERE app_id = OLD.app_id AND task_id = OLD.task_id;
END;
