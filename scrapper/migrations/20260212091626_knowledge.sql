CREATE TABLE IF NOT EXISTS topic (
	id INTEGER PRIMARY KEY,
	name VARCHAR(100) NOT NULL UNIQUE
);

-- table to store state for topic sequence struct between application runs  
CREATE TABLE IF NOT EXISTS topic_seq (
	next_id INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS entry (
	id INTEGER PRIMARY KEY,
	topic_id INTEGER NOT NULL,

	question TEXT NOT NULL UNIQUE,
	truth TEXT NOT NULL,

	added_at DATETIME NOT NULL,
	reviewed_at DATETIME,

	FOREIGN KEY(topic_id) REFERENCES knowledge_topic(id)
);
