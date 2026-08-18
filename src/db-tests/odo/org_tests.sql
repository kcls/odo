-- Tests for org.unit_ancestors, org.unit_descendants, and org.unit_full_path
-- These functions should return absolute depth values (root = 0)

BEGIN;

SELECT plan(43);

-- Shared org unit hierarchy (see seed/org_unit_data.sql for full tree)
\i seed/org_unit_data.sql

-- Suites 1-6 use the chain:
--   9000 (root, d0) → 9001 (region, d1) → 9010 (branch, d2) → 9020 (dept, d3)

-- =============================================================================
-- TEST SUITE 1: org.unit_ancestors - Returns ancestors with absolute depth
-- =============================================================================

-- Test 1.1: Ancestors of root should be just root at depth 0
SELECT results_eq(
    'SELECT id, depth FROM org.unit_ancestors(9000) ORDER BY depth',
    $$VALUES (9000, 0)$$,
    'Ancestors of root: root only at depth 0'
);

-- Test 1.2: Ancestors of region should be root and region
SELECT results_eq(
    'SELECT id, depth FROM org.unit_ancestors(9001) ORDER BY depth',
    $$VALUES (9000, 0), (9001, 1)$$,
    'Ancestors of region: root at depth 0, region at depth 1'
);

-- Test 1.3: Ancestors of branch should include all parents
SELECT results_eq(
    'SELECT id, depth FROM org.unit_ancestors(9010) ORDER BY depth',
    $$VALUES (9000, 0), (9001, 1), (9010, 2)$$,
    'Ancestors of branch: root(0), region(1), branch(2)'
);

-- Test 1.4: Ancestors of department should include all parents
SELECT results_eq(
    'SELECT id, depth FROM org.unit_ancestors(9020) ORDER BY depth',
    $$VALUES (9000, 0), (9001, 1), (9010, 2), (9020, 3)$$,
    'Ancestors of department: root(0), region(1), branch(2), dept(3)'
);

-- =============================================================================
-- TEST SUITE 2: org.unit_descendants - Returns descendants with absolute depth
-- =============================================================================

-- Test 2.1: Descendants of region should include both branches and departments
SELECT results_eq(
    'SELECT id, depth FROM org.unit_descendants(9001) WHERE id IN (9001,9010,9020,9011,9021) ORDER BY id',
    $$VALUES (9001, 1), (9010, 2), (9011, 2), (9020, 3), (9021, 3)$$,
    'Descendants of region: includes both branches and departments'
);

-- Test 2.2: Descendants of region (branch A chain): region, branch, dept
SELECT results_eq(
    'SELECT id, depth FROM org.unit_descendants(9001) WHERE id IN (9001,9010,9020) ORDER BY depth',
    $$VALUES (9001, 1), (9010, 2), (9020, 3)$$,
    'Descendants of region (branch A chain): region(1), branch(2), dept(3)'
);

-- Test 2.3: Descendants of branch should include branch and below
SELECT results_eq(
    'SELECT id, depth FROM org.unit_descendants(9010) ORDER BY depth',
    $$VALUES (9010, 2), (9020, 3)$$,
    'Descendants of branch: branch(2), dept(3)'
);

-- Test 2.4: Descendants of department should be just department
SELECT results_eq(
    'SELECT id, depth FROM org.unit_descendants(9020) ORDER BY depth',
    $$VALUES (9020, 3)$$,
    'Descendants of department: dept(3) only'
);

-- =============================================================================
-- TEST SUITE 3: org.unit_full_path - Returns ancestors + descendants
-- =============================================================================

-- Test 3.1: Full path of root includes all units
SELECT ok(
    (SELECT COUNT(*) FROM org.unit_full_path(9000)) >= 4,
    'Full path of root: includes at least the linear chain'
);

-- Test 3.2: Full path of region includes root + region + descendants
SELECT results_eq(
    'SELECT id, depth FROM org.unit_full_path(9001) WHERE id IN (9000,9001,9010,9020) ORDER BY depth',
    $$VALUES (9000, 0), (9001, 1), (9010, 2), (9020, 3)$$,
    'Full path of region: root + region + branch + dept'
);

-- Test 3.3: Full path of branch includes ancestors + branch + descendants
SELECT results_eq(
    'SELECT id, depth FROM org.unit_full_path(9010) WHERE id IN (9000,9001,9010,9020) ORDER BY depth',
    $$VALUES (9000, 0), (9001, 1), (9010, 2), (9020, 3)$$,
    'Full path of branch: ancestors + branch + descendants'
);

-- Test 3.4: Full path of department includes all ancestors + dept
SELECT results_eq(
    'SELECT id, depth FROM org.unit_full_path(9020) WHERE id IN (9000,9001,9010,9020) ORDER BY depth',
    $$VALUES (9000, 0), (9001, 1), (9010, 2), (9020, 3)$$,
    'Full path of department: all ancestors + dept'
);

-- =============================================================================
-- TEST SUITE 4: Multi-branch tree - Test with multiple children at same level
-- =============================================================================
-- Uses 9011 (Test Branch North-B) and 9021 (Test Dept North-B1) from shared seed

-- Test 4.1: Region descendants should include both branches
SELECT results_eq(
    'SELECT id, depth FROM org.unit_descendants(9001) WHERE id IN (9001,9010,9020,9011,9021) ORDER BY id',
    $$VALUES (9001, 1), (9010, 2), (9011, 2), (9020, 3), (9021, 3)$$,
    'Region descendants: includes both branches and their departments'
);

-- Test 4.2: Branch A descendants should not include branch B
SELECT results_eq(
    'SELECT id, depth FROM org.unit_descendants(9010) ORDER BY depth',
    $$VALUES (9010, 2), (9020, 3)$$,
    'Branch A descendants: only branch A and its department'
);

-- Test 4.3: Branch B descendants should not include branch A
SELECT results_eq(
    'SELECT id, depth FROM org.unit_descendants(9011) ORDER BY depth',
    $$VALUES (9011, 2), (9021, 3)$$,
    'Branch B descendants: only branch B and its department'
);

-- =============================================================================
-- TEST SUITE 5: Depth filtering scenarios for permissions
-- =============================================================================

-- Test 5.1: Units at depth >= 0 from root's full path (includes all test units)
SELECT ok(
    (SELECT COUNT(*) FROM org.unit_full_path(9000) WHERE depth >= 0) >= 6,
    'Root full path with min_depth 0: all units'
);

-- Test 5.2: Units at depth >= 1 from root's full path (exclude root)
SELECT ok(
    NOT EXISTS (SELECT 1 FROM org.unit_full_path(9000) WHERE depth >= 1 AND id = 9000),
    'Root full path with min_depth 1: excludes root'
);

-- Test 5.3: Units at depth >= 2 from root's full path (branches and below)
SELECT results_eq(
    'SELECT id FROM org.unit_full_path(9000) WHERE depth >= 2 AND id IN (9010,9020,9011,9021) ORDER BY id',
    $$VALUES (9010), (9011), (9020), (9021)$$,
    'Root full path with min_depth 2: branches and departments'
);

-- Test 5.4: Units at depth >= 1 from branch's full path
SELECT results_eq(
    'SELECT id FROM org.unit_full_path(9010) WHERE depth >= 1 AND id IN (9001,9010,9020) ORDER BY id',
    $$VALUES (9001), (9010), (9020)$$,
    'Branch full path with min_depth 1: excludes root'
);

-- Test 5.5: Units at depth >= 2 from branch's full path
SELECT results_eq(
    'SELECT id FROM org.unit_full_path(9010) WHERE depth >= 2 ORDER BY id',
    $$VALUES (9010), (9020)$$,
    'Branch full path with min_depth 2: branch and department only'
);

-- Test 5.6: Root should NOT be in branch's full path filtered by depth >= 1
SELECT ok(
    NOT EXISTS (SELECT 1 FROM org.unit_full_path(9010) WHERE depth >= 1 AND id = 9000),
    'Branch full path with min_depth 1: root (depth 0) is excluded'
);

-- =============================================================================
-- TEST SUITE 6: Edge cases
-- =============================================================================

-- Test 6.1: Verify all nodes include themselves
SELECT ok(
    EXISTS (SELECT 1 FROM org.unit_ancestors(9020) WHERE id = 9020),
    'unit_ancestors includes the starting node'
);

SELECT ok(
    EXISTS (SELECT 1 FROM org.unit_descendants(9020) WHERE id = 9020),
    'unit_descendants includes the starting node'
);

SELECT ok(
    EXISTS (SELECT 1 FROM org.unit_full_path(9020) WHERE id = 9020),
    'unit_full_path includes the starting node'
);

-- Test 6.2: Verify no duplicates in full_path
SELECT results_eq(
    'SELECT COUNT(*), id FROM org.unit_full_path(9010) GROUP BY id HAVING COUNT(*) > 1',
    'SELECT NULL::bigint, NULL::integer WHERE false',
    'unit_full_path has no duplicate nodes'
);

-- Test 6.3: Root depth is always 0
SELECT is(
    (SELECT depth FROM org.unit_ancestors(9020) WHERE id = 9000),
    0,
    'Root always has depth 0 in ancestors'
);

SELECT is(
    (SELECT depth FROM org.unit_descendants(9000) WHERE id = 9000),
    0,
    'Root always has depth 0 in descendants'
);

-- =============================================================================
-- TEST SUITE 7: org.unit_full_path_at_depth - Sibling expansion
-- =============================================================================
-- Uses the full tree from shared seed:
--   9001: Region North    9002: Region South
--   9010: Branch N-A      9011: Branch N-B      9012: Branch S-A    9013: Branch S-B
--   9020: Dept N-A1       9021: Dept N-B1       9022: Dept S-A1

-- Test 7.1: Start at Branch North-A, target depth 0 - returns entire tree
SELECT ok(
    (SELECT COUNT(*) FROM org.unit_full_path_at_depth(9010, 0)) = (SELECT COUNT(*) FROM org.unit_full_path(9000)),
    'From Branch North-A to depth 0: returns entire tree'
);

-- Test 7.2: Start at Branch North-A, target depth 1 - returns Region North and descendants
SELECT results_eq(
    'SELECT id FROM org.unit_full_path_at_depth(9010, 1) ORDER BY id',
    $$VALUES (9001), (9010), (9011), (9020), (9021)$$,
    'From Branch North-A to depth 1: returns Region North and descendants (includes sibling Branch North-B)'
);

-- Test 7.3: Start at Branch North-A, target depth 2 - returns Branch North-A and descendants only
SELECT results_eq(
    'SELECT id FROM org.unit_full_path_at_depth(9010, 2) ORDER BY id',
    $$VALUES (9010), (9020)$$,
    'From Branch North-A to depth 2: returns Branch North-A and its department only'
);

-- Test 7.4: Branch North-A at depth 1 should include sibling Branch North-B
SELECT ok(
    EXISTS (SELECT 1 FROM org.unit_full_path_at_depth(9010, 1) WHERE id = 9011),
    'Branch North-A at depth 1 includes sibling Branch North-B'
);

-- Test 7.5: Branch North-A at depth 1 should NOT include Branch South-A (different region)
SELECT ok(
    NOT EXISTS (SELECT 1 FROM org.unit_full_path_at_depth(9010, 1) WHERE id = 9012),
    'Branch North-A at depth 1 excludes Branch South-A from different region'
);

-- Test 7.6: Branch North-B at depth 1 should include sibling Branch North-A
SELECT ok(
    EXISTS (SELECT 1 FROM org.unit_full_path_at_depth(9011, 1) WHERE id = 9010),
    'Branch North-B at depth 1 includes sibling Branch North-A'
);

-- Test 7.7: Branch South-A at depth 0 should include branches from all regions
SELECT ok(
    EXISTS (SELECT 1 FROM org.unit_full_path_at_depth(9012, 0) WHERE id = 9010) AND
    EXISTS (SELECT 1 FROM org.unit_full_path_at_depth(9012, 0) WHERE id = 9011),
    'Branch South-A at depth 0 includes branches from all regions'
);

-- Test 7.8: All returned units should have depth >= target_depth (depth 1)
SELECT ok(
    NOT EXISTS (
        SELECT 1 FROM org.unit_full_path_at_depth(9010, 1)
        WHERE depth < 1
    ),
    'Full path at depth 1: all units have depth >= 1'
);

-- Test 7.9: All returned units should have depth >= target_depth (depth 2)
SELECT ok(
    NOT EXISTS (
        SELECT 1 FROM org.unit_full_path_at_depth(9010, 2)
        WHERE depth < 2
    ),
    'Full path at depth 2: all units have depth >= 2'
);

-- Test 7.10: Verify specific depths in result set (depth 1)
SELECT results_eq(
    'SELECT DISTINCT depth FROM org.unit_full_path_at_depth(9010, 1) ORDER BY depth',
    $$VALUES (1), (2), (3)$$,
    'Full path at depth 1 contains depths 1, 2, 3'
);

-- Test 7.11: Verify specific depths in result set (depth 0)
SELECT results_eq(
    'SELECT DISTINCT depth FROM org.unit_full_path_at_depth(9010, 0) ORDER BY depth',
    $$VALUES (0), (1), (2), (3)$$,
    'Full path at depth 0 contains depths 0, 1, 2, 3'
);

-- Test 7.12: min_depth 0 from Branch North-A includes root
SELECT ok(
    EXISTS (SELECT 1 FROM org.unit_full_path_at_depth(9010, 0) WHERE id = 9000),
    'min_depth 0 from Branch North-A includes root'
);

-- Test 7.13: min_depth 0 from Dept North-A1 includes root
SELECT ok(
    EXISTS (SELECT 1 FROM org.unit_full_path_at_depth(9020, 0) WHERE id = 9000),
    'min_depth 0 from Dept North-A1 includes root'
);

-- Test 7.14: Role at Branch North-A with min_depth 1 includes Region North
SELECT ok(
    EXISTS (SELECT 1 FROM org.unit_full_path_at_depth(9010, 1) WHERE id = 9001),
    'Role at Branch North-A with min_depth 1 includes Region North'
);

-- Test 7.15: Role at Branch North-A with min_depth 1 includes sibling Branch North-B
SELECT ok(
    EXISTS (SELECT 1 FROM org.unit_full_path_at_depth(9010, 1) WHERE id = 9011),
    'Role at Branch North-A with min_depth 1 includes sibling Branch North-B'
);

-- Test 7.16: Dept North-A1 at depth 3 returns only itself
SELECT results_eq(
    'SELECT id FROM org.unit_full_path_at_depth(9020, 3) ORDER BY id',
    $$VALUES (9020)$$,
    'From Dept North-A1 to depth 3: returns only Dept North-A1'
);

SELECT * FROM finish();

ROLLBACK;
