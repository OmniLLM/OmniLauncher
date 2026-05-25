ALTER TABLE todos ADD COLUMN status TEXT NOT NULL DEFAULT 'todo';
UPDATE todos
SET status = CASE
    WHEN done = 1 THEN 'done'
    ELSE 'todo'
END
WHERE status IS NULL OR status = '' OR status = 'todo';
