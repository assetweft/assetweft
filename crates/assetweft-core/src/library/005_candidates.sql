-- M2 immutable BYTE candidates and explicitly fixture-only conditional current revisions.
CREATE TABLE aw_candidates (
 id TEXT PRIMARY KEY NOT NULL,
 base_version TEXT NOT NULL REFERENCES aw_asset_versions(version_id),
 candidate_version TEXT NOT NULL REFERENCES aw_asset_versions(version_id),
 descriptor TEXT NOT NULL CHECK(length(descriptor)<=32768)
);
CREATE TRIGGER aw_candidates_immutable_update BEFORE UPDATE ON aw_candidates
BEGIN SELECT RAISE(ABORT,'immutable candidate'); END;
CREATE TRIGGER aw_candidates_immutable_delete BEFORE DELETE ON aw_candidates
BEGIN SELECT RAISE(ABORT,'immutable candidate'); END;
CREATE TRIGGER aw_candidate_refs_frozen_insert BEFORE INSERT ON aw_component_references
WHEN EXISTS(SELECT 1 FROM aw_candidates WHERE base_version=NEW.owner_version_id OR candidate_version=NEW.owner_version_id)
BEGIN SELECT RAISE(ABORT,'candidate references frozen'); END;
CREATE TRIGGER aw_candidate_refs_frozen_update BEFORE UPDATE ON aw_component_references
WHEN EXISTS(SELECT 1 FROM aw_candidates WHERE base_version IN (OLD.owner_version_id,NEW.owner_version_id) OR candidate_version IN (OLD.owner_version_id,NEW.owner_version_id))
BEGIN SELECT RAISE(ABORT,'candidate references frozen'); END;
CREATE TRIGGER aw_candidate_refs_frozen_delete BEFORE DELETE ON aw_component_references
WHEN EXISTS(SELECT 1 FROM aw_candidates WHERE base_version=OLD.owner_version_id OR candidate_version=OLD.owner_version_id)
BEGIN SELECT RAISE(ABORT,'candidate references frozen'); END;
-- Explicit fixture structure scope; contract is frozen for this asset's local head.
CREATE TABLE aw_candidate_current (
 asset_id TEXT PRIMARY KEY NOT NULL REFERENCES aw_assets(asset_id),
 version_id TEXT NOT NULL REFERENCES aw_asset_versions(version_id),
 revision INTEGER NOT NULL CHECK(typeof(revision)='integer' AND revision>=0),
 contract TEXT NOT NULL
);
CREATE TABLE aw_candidate_attempts (
 candidate_id TEXT PRIMARY KEY NOT NULL REFERENCES aw_candidates(id),
 receipt TEXT NOT NULL
);
CREATE TRIGGER aw_candidate_current_contract BEFORE UPDATE OF contract,asset_id ON aw_candidate_current
BEGIN SELECT RAISE(ABORT,'immutable current contract'); END;
CREATE TRIGGER aw_candidate_attempt_update BEFORE UPDATE ON aw_candidate_attempts
BEGIN SELECT RAISE(ABORT,'immutable attempt'); END;
CREATE TRIGGER aw_candidate_attempt_delete BEFORE DELETE ON aw_candidate_attempts
BEGIN SELECT RAISE(ABORT,'immutable attempt'); END;
CREATE TRIGGER aw_current_refs_insert BEFORE INSERT ON aw_component_references
WHEN EXISTS(SELECT 1 FROM aw_candidate_current WHERE version_id=NEW.owner_version_id)
BEGIN SELECT RAISE(ABORT,'current references frozen'); END;
CREATE TRIGGER aw_current_refs_update BEFORE UPDATE ON aw_component_references
WHEN EXISTS(SELECT 1 FROM aw_candidate_current WHERE version_id IN (OLD.owner_version_id,NEW.owner_version_id))
BEGIN SELECT RAISE(ABORT,'current references frozen'); END;
CREATE TRIGGER aw_current_refs_delete BEFORE DELETE ON aw_component_references
WHEN EXISTS(SELECT 1 FROM aw_candidate_current WHERE version_id=OLD.owner_version_id)
BEGIN SELECT RAISE(ABORT,'current references frozen'); END;
