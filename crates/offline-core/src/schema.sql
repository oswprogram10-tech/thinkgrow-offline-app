CREATE TABLE meta(key TEXT PRIMARY KEY,value BLOB NOT NULL);
CREATE TABLE counters(seq INTEGER NOT NULL,wall_floor INTEGER NOT NULL);
INSERT INTO counters VALUES(0,0);
CREATE TABLE drafts(assignment TEXT PRIMARY KEY,revision INTEGER NOT NULL,dirty INTEGER NOT NULL);
CREATE TABLE draft_revisions(assignment TEXT NOT NULL,revision INTEGER NOT NULL,cipher BLOB NOT NULL,PRIMARY KEY(assignment,revision));
CREATE TABLE outbox(operation_id TEXT PRIMARY KEY,intent TEXT NOT NULL UNIQUE,cipher BLOB NOT NULL);
CREATE TABLE deliveries(operation_id TEXT NOT NULL REFERENCES outbox(operation_id),destination TEXT NOT NULL,
 state TEXT NOT NULL,attempts INTEGER NOT NULL DEFAULT 0,next_ms INTEGER NOT NULL DEFAULT 0,
 lease TEXT,lease_until INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(operation_id,destination));
CREATE TABLE receipts(operation_id TEXT NOT NULL REFERENCES outbox(operation_id),destination TEXT NOT NULL,signed TEXT NOT NULL,
 PRIMARY KEY(operation_id,destination));
CREATE TABLE conflicts(id INTEGER PRIMARY KEY,operation_id TEXT NOT NULL REFERENCES outbox(operation_id),cipher BLOB NOT NULL);
CREATE TABLE blobs(hash TEXT PRIMARY KEY,state TEXT NOT NULL);
PRAGMA user_version=1;
