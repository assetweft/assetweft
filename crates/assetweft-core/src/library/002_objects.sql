-- AW010 managed bytes are separate from AW009 unchecked/external metadata.
CREATE TABLE aw_objects (
 content_hash TEXT PRIMARY KEY NOT NULL CHECK(length(content_hash)=64 AND content_hash NOT GLOB '*[^0-9a-f]*'),
 byte_length INTEGER NOT NULL CHECK(byte_length>=0),
 backup_pin INTEGER NOT NULL DEFAULT 0 CHECK(backup_pin IN (0,1)),
 unsynced_unique INTEGER NOT NULL DEFAULT 1 CHECK(unsynced_unique IN (0,1))
);
CREATE TABLE aw_managed_versions (
 version_id TEXT PRIMARY KEY NOT NULL REFERENCES aw_asset_versions(version_id),
 content_hash TEXT NOT NULL REFERENCES aw_objects(content_hash)
);
CREATE TRIGGER aw_managed_hash BEFORE INSERT ON aw_managed_versions
WHEN NOT EXISTS(SELECT 1 FROM aw_asset_versions v WHERE v.version_id=NEW.version_id AND v.content_hash=NEW.content_hash)
BEGIN SELECT RAISE(ABORT,'managed version hash mismatch'); END;
