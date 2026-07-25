DELETE FROM song_plays 
WHERE track_uuid IN (
    SELECT track_uuid FROM tracks 
    WHERE title IS NULL 
       OR artist IS NULL 
       OR source_url IS NULL 
       OR duration_sec IS NULL 
       OR thumbnail_url IS NULL
);

DELETE FROM tracks 
WHERE title IS NULL 
   OR artist IS NULL 
   OR source_url IS NULL 
   OR duration_sec IS NULL 
   OR thumbnail_url IS NULL;

ALTER TABLE song_plays 
    ALTER COLUMN requested_by SET NOT NULL;

ALTER TABLE tracks 
    ALTER COLUMN title SET NOT NULL,
    ALTER COLUMN artist SET NOT NULL,
    ALTER COLUMN source_url SET NOT NULL,
    ALTER COLUMN duration_sec SET NOT NULL,
    ALTER COLUMN thumbnail_url SET NOT NULL;
