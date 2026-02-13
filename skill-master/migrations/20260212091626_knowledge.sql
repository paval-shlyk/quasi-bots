CREATE TABLE IF NOT EXISTS topic (
	id INTEGER PRIMARY KEY,
	name VARCHAR(100) NOT NULL UNIQUE,
	is_used BOOLEAN NOT NULL DEFAULT FALSE,
	disabled_until DATETIME DEFAULT NULL,

	affinity_days INTEGER DEFAULT NULL
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

CREATE TRIGGER IF NOT EXISTS trg_mark_entry_reviewed
AFTER UPDATE OF is_reviewed ON entry
WHEN OLD.is_reviewed = FALSE AND NEW.is_reviewed = TRUE
BEGIN
    DELETE FROM review WHERE entry_id = NEW.id; --ensure only one review record per entry
    INSERT INTO review (entry_id, attempts) VALUES (NEW.id, 0);
END;
