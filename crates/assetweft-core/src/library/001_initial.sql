-- AW-009: first product schema, metadata only. Applied atomically with its version row.
CREATE TABLE aw_assets (
    asset_id TEXT PRIMARY KEY NOT NULL CHECK (length(asset_id) > 0)
);
CREATE TABLE aw_asset_versions (
    version_id TEXT PRIMARY KEY NOT NULL CHECK (length(version_id) > 0),
    asset_id TEXT NOT NULL REFERENCES aw_assets(asset_id),
    content_hash TEXT NOT NULL CHECK (length(content_hash) = 64 AND content_hash NOT GLOB '*[^0-9a-f]*'),
    input_id TEXT NOT NULL CHECK (length(input_id) > 0),
    source TEXT NOT NULL CHECK (length(source) > 0),
    authorization TEXT NOT NULL CHECK (length(authorization) > 0),
    authorized INTEGER NOT NULL CHECK (authorized IN (0, 1)),
    license_id TEXT NOT NULL CHECK (length(license_id) > 0),
    usage_scope TEXT NOT NULL CHECK (length(usage_scope) > 0),
    source_format TEXT NOT NULL CHECK (length(source_format) > 0),
    contract_version TEXT NOT NULL CHECK (length(contract_version) > 0),
    presence TEXT NOT NULL CHECK (presence IN ('UNKNOWN', 'MISSING'))
);
CREATE TABLE aw_component_references (
    owner_version_id TEXT NOT NULL REFERENCES aw_asset_versions(version_id),
    role TEXT NOT NULL CHECK (role IN ('mesh','materials','rig','motion','vfx','audio','dialogue')),
    component_version_id TEXT NOT NULL REFERENCES aw_asset_versions(version_id),
    PRIMARY KEY (owner_version_id, role, component_version_id)
);
