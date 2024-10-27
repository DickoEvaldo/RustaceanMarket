-- In migrations/[timestamp]_make_email_unique.sql
-- Up Migration (what we want to do)
ALTER TABLE users
    ADD CONSTRAINT users_email_unique UNIQUE (email);

-- Down Migration (how to undo it)
ALTER TABLE users
    DROP CONSTRAINT users_email_unique;