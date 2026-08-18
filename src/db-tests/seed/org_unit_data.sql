-- Shared test org unit hierarchy.
-- Included via \i from individual test files (within their BEGIN/ROLLBACK).
--
-- Depth 0: Test Root (9000)
-- Depth 1: Test Region North (9001)
--           Test Region South (9002)
-- Depth 2: Test Branch North-A (9010) — child of Test Region North
--           Test Branch North-B (9011) — child of Test Region North
--           Test Branch South-A (9012) — child of Test Region South
--           Test Branch South-B (9013) — child of Test Region South
-- Depth 3: Test Dept North-A1 (9020)  — child of Test Branch North-A
--           Test Dept North-B1 (9021)  — child of Test Branch North-B
--           Test Dept South-A1 (9022)  — child of Test Branch South-A

-- A single live root is enforced by org.unit's unit_single_root partial index
-- (migration 091). Soft-delete any existing live root so this seed's detached
-- tree (rooted at 9000) is the sole live root; org.root() filters deleted_at,
-- so it now resolves to 9000. Everything rolls back at end of the test tx.
UPDATE org.unit SET deleted_at = now()
    WHERE parent IS NULL AND deleted_at IS NULL;

-- Unit types resolved by depth, not id: the seed data (KCLS-historical or
-- the generic platform demo) generates unit_type ids, so nothing here may
-- hard-code them. Any 4-deep type chain works for these tests.
INSERT INTO org.unit (id, label, code, parent, unit_type) VALUES
    (9000, 'Test Root',             'TEST-ROOT',       NULL,
     (SELECT id FROM org.unit_type WHERE parent IS NULL LIMIT 1)),

    (9001, 'Test Region North',     'TEST-NORTH',      9000,
     (SELECT t.id FROM org.unit_type t JOIN org.unit_type p ON t.parent = p.id
       WHERE p.parent IS NULL LIMIT 1)),
    (9002, 'Test Region South',     'TEST-SOUTH',      9000,
     (SELECT t.id FROM org.unit_type t JOIN org.unit_type p ON t.parent = p.id
       WHERE p.parent IS NULL LIMIT 1)),

    (9010, 'Test Branch North-A',   'TEST-BRANCH-NA',  9001,
     (SELECT t.id FROM org.unit_type t JOIN org.unit_type p ON t.parent = p.id
       JOIN org.unit_type g ON p.parent = g.id WHERE g.parent IS NULL LIMIT 1)),
    (9011, 'Test Branch North-B',   'TEST-BRANCH-NB',  9001,
     (SELECT t.id FROM org.unit_type t JOIN org.unit_type p ON t.parent = p.id
       JOIN org.unit_type g ON p.parent = g.id WHERE g.parent IS NULL LIMIT 1)),
    (9012, 'Test Branch South-A',   'TEST-BRANCH-SA',  9002,
     (SELECT t.id FROM org.unit_type t JOIN org.unit_type p ON t.parent = p.id
       JOIN org.unit_type g ON p.parent = g.id WHERE g.parent IS NULL LIMIT 1)),
    (9013, 'Test Branch South-B',   'TEST-BRANCH-SB',  9002,
     (SELECT t.id FROM org.unit_type t JOIN org.unit_type p ON t.parent = p.id
       JOIN org.unit_type g ON p.parent = g.id WHERE g.parent IS NULL LIMIT 1)),

    (9020, 'Test Dept North-A1',    'TEST-DEPT-NA1',   9010,
     (SELECT t.id FROM org.unit_type t JOIN org.unit_type p ON t.parent = p.id
       JOIN org.unit_type g ON p.parent = g.id JOIN org.unit_type r ON g.parent = r.id
       WHERE r.parent IS NULL LIMIT 1)),
    (9021, 'Test Dept North-B1',    'TEST-DEPT-NB1',   9011,
     (SELECT t.id FROM org.unit_type t JOIN org.unit_type p ON t.parent = p.id
       JOIN org.unit_type g ON p.parent = g.id JOIN org.unit_type r ON g.parent = r.id
       WHERE r.parent IS NULL LIMIT 1)),
    (9022, 'Test Dept South-A1',    'TEST-DEPT-SA1',   9012,
     (SELECT t.id FROM org.unit_type t JOIN org.unit_type p ON t.parent = p.id
       JOIN org.unit_type g ON p.parent = g.id JOIN org.unit_type r ON g.parent = r.id
       WHERE r.parent IS NULL LIMIT 1));
