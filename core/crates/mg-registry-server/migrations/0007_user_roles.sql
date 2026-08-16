-- user roles (task #4): admin|publisher|viewer
ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'viewer';
UPDATE users SET role = 'admin' WHERE is_admin = 1;