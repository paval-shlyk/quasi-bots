CREATE TABLE IF NOT EXISTS article (
	id INTEGER PRIMARY KEY,
	topic_id INTEGER NOT NULL,

	title TEXT NOT NULL,
	content TEXT DEFAULT NULL, --content is null if the article is not fully scraped yet, but we want to save the metadata (title, authors, links, published_at) for later processing

	authors TEXT NOT NULL CHECK (json_valid(authors) == TRUE),
	links TEXT NOT NULL CHECK (json_valid(links) == TRUE),

	published_at DATETIME NOT NULL,

	UNIQUE(topic_id, published_at, title),
	FOREIGN KEY(topic_id) REFERENCES news_topic(id)
);

CREATE TABLE IF NOT EXISTS news_topic (
	id INTEGER PRIMARY KEY,
	name VARCHAR(100) NOT NULL UNIQUE
);



-- todo: use mapping with source,authors and links... But for what reason?
