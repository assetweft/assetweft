-- M1 append-only local feedback. Sequence is local ordering, never a sync identity.
CREATE TABLE aw_feedback (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 feedback_id TEXT NOT NULL UNIQUE CHECK(length(CAST(feedback_id AS BLOB)) BETWEEN 1 AND 256),
 asset_id TEXT NOT NULL REFERENCES aw_assets(asset_id),
 version_id TEXT NOT NULL REFERENCES aw_asset_versions(version_id),
 decision TEXT NOT NULL CHECK(decision IN ('ACCEPT','REJECT')),
 reason TEXT CHECK(reason IS NULL OR length(CAST(reason AS BLOB)) BETWEEN 1 AND 2048)
) STRICT;
CREATE INDEX aw_feedback_asset_order ON aw_feedback(asset_id, sequence DESC);
CREATE INDEX aw_feedback_version_order ON aw_feedback(asset_id, version_id, sequence DESC);
CREATE TRIGGER aw_feedback_version_binding BEFORE INSERT ON aw_feedback
WHEN NOT EXISTS(SELECT 1 FROM aw_asset_versions WHERE version_id=NEW.version_id AND asset_id=NEW.asset_id)
BEGIN SELECT RAISE(ABORT,'feedback version/asset mismatch'); END;
CREATE TRIGGER aw_feedback_update BEFORE UPDATE ON aw_feedback
BEGIN SELECT RAISE(ABORT,'immutable feedback'); END;
CREATE TRIGGER aw_feedback_delete BEFORE DELETE ON aw_feedback
BEGIN SELECT RAISE(ABORT,'immutable feedback'); END;
