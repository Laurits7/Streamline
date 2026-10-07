-- Project descriptions and ideas (D-72). An idea is a project that isn't started yet:
-- its tasks ask for no attention until it's activated. Status applies to a whole subtree.
ALTER TABLE projects ADD COLUMN description TEXT NOT NULL DEFAULT '';
ALTER TABLE projects ADD COLUMN status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'idea'));
