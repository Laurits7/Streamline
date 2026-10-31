-- Assigning household chores to a member (D-116). `tasks.assignee_user_id` exists since
-- the start (Q-4); routines and multi-step chores get a default assignee and an optional
-- rotation: `rotation_user_ids` is a JSON array of user ids taking turns (empty = off).
ALTER TABLE series ADD COLUMN assignee_user_id TEXT;
ALTER TABLE series ADD COLUMN rotation_user_ids TEXT NOT NULL DEFAULT '[]';

-- Multi-step chores take turns per run: `rotation_turn` counts the runs started so far.
ALTER TABLE workflow_templates ADD COLUMN assignee_user_id TEXT;
ALTER TABLE workflow_templates ADD COLUMN rotation_user_ids TEXT NOT NULL DEFAULT '[]';
ALTER TABLE workflow_templates ADD COLUMN rotation_turn INTEGER NOT NULL DEFAULT 0;

CREATE INDEX tasks_assignee ON tasks (assignee_user_id) WHERE assignee_user_id IS NOT NULL;

-- Things in a project belong to its owner (D-115). Multi-step chores never followed their
-- project, and routines didn't when a project was shared after they were made (the owner
-- wasn't written on update): put them right, with a new revision so clients pick them up.
-- Projects copied from another server are left alone (their groups are mirrors).
UPDATE meta_rev SET rev = rev + 1 WHERE id = 1;
UPDATE workflow_templates
   SET owner_user_id = (SELECT p.owner_user_id FROM projects p WHERE p.id = workflow_templates.project_id),
       owner_group_id = (SELECT p.owner_group_id FROM projects p WHERE p.id = workflow_templates.project_id),
       rev = (SELECT rev FROM meta_rev WHERE id = 1)
 WHERE deleted_at IS NULL
   AND project_id IN (SELECT p.id FROM projects p LEFT JOIN groups g ON g.id = p.owner_group_id
                       WHERE p.deleted_at IS NULL AND g.home IS NULL)
   AND (owner_user_id IS NOT (SELECT p.owner_user_id FROM projects p WHERE p.id = workflow_templates.project_id)
     OR owner_group_id IS NOT (SELECT p.owner_group_id FROM projects p WHERE p.id = workflow_templates.project_id));
UPDATE series
   SET owner_user_id = (SELECT p.owner_user_id FROM projects p WHERE p.id = series.project_id),
       owner_group_id = (SELECT p.owner_group_id FROM projects p WHERE p.id = series.project_id),
       rev = (SELECT rev FROM meta_rev WHERE id = 1)
 WHERE deleted_at IS NULL
   AND project_id IN (SELECT p.id FROM projects p LEFT JOIN groups g ON g.id = p.owner_group_id
                       WHERE p.deleted_at IS NULL AND g.home IS NULL)
   AND (owner_user_id IS NOT (SELECT p.owner_user_id FROM projects p WHERE p.id = series.project_id)
     OR owner_group_id IS NOT (SELECT p.owner_group_id FROM projects p WHERE p.id = series.project_id));
