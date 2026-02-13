CREATE TABLE IF NOT EXISTS topic (
	id INTEGER PRIMARY KEY,
	name VARCHAR(100) NOT NULL UNIQUE,
	is_used BOOLEAN NOT NULL DEFAULT FALSE,
	disabled_until DATETIME DEFAULT NULL,

	affinity_days INTEGER DEFAULT NULL CHECK (affinity_days > 0)
);

CREATE TABLE IF NOT EXISTS entry (
	id INTEGER PRIMARY KEY,
	topic_id INTEGER NOT NULL,
	--unique question string identifier
	name VARCHAR(100) NOT NULL UNIQUE,

	question TEXT NOT NULL UNIQUE,
	truth TEXT NOT NULL,

	added_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
	
	is_reviewed BOOLEAN NOT NULL DEFAULT FALSE,

	affinity_days INTEGER NOT NULL DEFAULT 0,

	FOREIGN KEY(topic_id) REFERENCES topic(id)
);

CREATE TABLE IF NOT EXISTS tag (
	id INTEGER PRIMARY KEY,
	name VARCHAR(100) NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS m2m_entry_tag (
	entry_id INTEGER NOT NULL,
	tag_id INTEGER NOT NULL,

	PRIMARY KEY (entry_id, tag_id),

	FOREIGN KEY(entry_id) REFERENCES entry(id),
	FOREIGN KEY(tag_id) REFERENCES tag(id)
);

CREATE TABLE IF NOT EXISTS review (
	id INTEGER PRIMARY KEY,
	entry_id INTEGER UNIQUE NOT NULL,
	reviewed_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
	attempts INTEGER NOT NULL,

	FOREIGN KEY(entry_id) REFERENCES entry(id)
);

CREATE TRIGGER trg_mark_entry_reviewed
AFTER UPDATE OF is_reviewed ON entry
WHEN OLD.is_reviewed = FALSE AND NEW.is_reviewed = TRUE
BEGIN
    DELETE FROM review WHERE entry_id = NEW.id; --ensure only one review record per entry
    INSERT INTO review (entry_id, attempts) VALUES (NEW.id, 0);
END;

CREATE TRIGGER trg_mark_topic_disabled
AFTER UPDATE OF is_used ON topic
WHEN OLD.is_used = FALSE AND NEW.is_used = TRUE AND NEW.affinity_days IS NOT NULL
BEGIN
    UPDATE topic
    SET disabled_until = datetime('now', '+' || NEW.affinity_days || ' days')
    WHERE id = NEW.id;
END;

CREATE TRIGGER trg_mark_topic_enabled
AFTER UPDATE OF affinity_days ON topic
WHEN OLD.affinity_days IS NOT NULL AND NEW.affinity_days IS NULL AND NEW.disabled_until IS NOT NULL
BEGIN
    UPDATE topic
    SET disabled_until = NULL
    WHERE id = NEW.id;
END;
