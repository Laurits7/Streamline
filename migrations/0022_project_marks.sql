-- Project marks (D-74): every project has a colour and a shape. Existing projects get
-- them here, in the order `streamline_domain::marks` hands them out: top-level ones per
-- owner by position (keeping a colour already chosen), subprojects take their top-level
-- project's colour and a shape by position among their siblings.
ALTER TABLE projects ADD COLUMN shape TEXT NOT NULL DEFAULT 'circle'
    CHECK (shape IN ('circle', 'square', 'triangle', 'diamond', 'hexagon', 'star'));

CREATE TEMP TABLE palette (i INTEGER PRIMARY KEY, color TEXT NOT NULL);
INSERT INTO palette VALUES (0, '#dc2626'), (1, '#1e66cc'), (2, '#16a34a'), (3, '#ea580c'),
    (4, '#7c3aed'), (5, '#0891b2'), (6, '#db2777'), (7, '#ca8a04'), (8, '#4f46e5');
CREATE TEMP TABLE shapes (i INTEGER PRIMARY KEY, shape TEXT NOT NULL);
INSERT INTO shapes VALUES (0, 'circle'), (1, 'square'), (2, 'triangle'), (3, 'diamond'),
    (4, 'hexagon'), (5, 'star');

UPDATE meta_rev SET rev = rev + 1 WHERE id = 1;

-- Top level: the n-th project gets colour n % 9 and shape (n % 9 + n / 9) % 6.
CREATE TEMP TABLE top_rank AS
SELECT id, (ROW_NUMBER() OVER (PARTITION BY COALESCE(owner_group_id, owner_user_id) ORDER BY position, created_at) - 1) % 54 AS n
FROM projects WHERE parent_id IS NULL AND deleted_at IS NULL;

UPDATE projects SET
    color = COALESCE(color, (SELECT p.color FROM top_rank r JOIN palette p ON p.i = r.n % 9 WHERE r.id = projects.id)),
    shape = (SELECT s.shape FROM top_rank r JOIN shapes s ON s.i = (r.n % 9 + r.n / 9) % 6 WHERE r.id = projects.id),
    rev = (SELECT rev FROM meta_rev WHERE id = 1)
WHERE id IN (SELECT id FROM top_rank);

-- Subprojects, at any depth, walking down from the top: the parent's colour (unless one
-- was chosen), and the shapes after the parent's in turn by position among siblings, so
-- a subproject never looks like its parent (unless it has six or more siblings).
CREATE TEMP TABLE sub_mark AS
WITH RECURSIVE
ranked AS (
    SELECT id, parent_id, color,
        ROW_NUMBER() OVER (PARTITION BY parent_id ORDER BY position, created_at) AS n
    FROM projects WHERE parent_id IS NOT NULL AND deleted_at IS NULL
),
walk(id, color, si) AS (
    SELECT p.id, p.color, s.i FROM projects p JOIN shapes s ON s.shape = p.shape WHERE p.parent_id IS NULL
    UNION ALL
    SELECT r.id, COALESCE(r.color, w.color), (w.si + r.n) % 6 FROM ranked r JOIN walk w ON r.parent_id = w.id
)
SELECT w.id, w.color, s.shape FROM walk w JOIN shapes s ON s.i = w.si
WHERE w.id IN (SELECT id FROM ranked);

UPDATE projects SET
    color = (SELECT color FROM sub_mark m WHERE m.id = projects.id),
    shape = (SELECT shape FROM sub_mark m WHERE m.id = projects.id),
    rev = (SELECT rev FROM meta_rev WHERE id = 1)
WHERE id IN (SELECT id FROM sub_mark);

DROP TABLE top_rank;
DROP TABLE sub_mark;
DROP TABLE palette;
DROP TABLE shapes;
