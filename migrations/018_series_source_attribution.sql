-- Optional attribution for manually registered series (hidden until set).
ALTER TABLE series ADD COLUMN source_label TEXT;
ALTER TABLE series ADD COLUMN source_url TEXT;
