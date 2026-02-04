-- Add cost columns to requests table
-- These columns store the calculated cost for each request

ALTER TABLE requests ADD COLUMN input_cost REAL NOT NULL DEFAULT 0.0;
ALTER TABLE requests ADD COLUMN output_cost REAL NOT NULL DEFAULT 0.0;
ALTER TABLE requests ADD COLUMN cache_write_cost REAL NOT NULL DEFAULT 0.0;
ALTER TABLE requests ADD COLUMN cache_read_cost REAL NOT NULL DEFAULT 0.0;
ALTER TABLE requests ADD COLUMN total_cost REAL NOT NULL DEFAULT 0.0;

-- Add indexes to support cost aggregation queries
CREATE INDEX IF NOT EXISTS idx_requests_total_cost ON requests(total_cost);
CREATE INDEX IF NOT EXISTS idx_requests_model_cost ON requests(model, total_cost);
