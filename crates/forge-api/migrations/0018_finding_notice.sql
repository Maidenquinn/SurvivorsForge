-- "notice": shown like a warning, but does not make a version flagged (e.g. a web address).
ALTER TABLE findings DROP CONSTRAINT findings_severity_check;
ALTER TABLE findings ADD CONSTRAINT findings_severity_check CHECK (severity IN ('info', 'notice', 'warn', 'critical'));
