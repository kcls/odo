-- Deploy odo-demo:001_demo_org_tree to pg
-- requires: odo:002_odo_seed

-- A small sample org tree beneath the seeded root, exercising every unit
-- type:
--
--   <root>
--   |-- East Region (ERG)
--   |   |-- Main Street Branch (MAIN)
--   |   |   `-- Main Street Locker (MAINL)
--   |   `-- Riverside Branch (RIVR)
--   `-- West Region (WRG)
--       |-- Hilltop Branch (HILL)
--       `-- Lakeside Branch (LAKE)
--
-- This is demo data. A public install wants it -- it is what the README
-- walkthrough, the e2e suites and a new developer's checkout all assume
-- exists. An installation deploying into its own org structure does not:
-- it deploys the `odo` project alone and registers its real tree through
-- odo-register manifests.
--
-- The root itself is seeded by odo:002_odo_seed, which parameterizes its
-- code. The two regions attach to whatever that produced, so this tree
-- hangs correctly under a renamed root -- though an installation that
-- renamed its root almost certainly does not want this project at all.
\if :{?root_code}
\else
  \set root_code OLS
\endif

-- Parents resolved by code; no hard-coded ids.
BEGIN;

INSERT INTO org.unit (label, code, parent, unit_type, timezone, uuid) VALUES
    ('East Region', 'ERG', (SELECT id FROM org.unit WHERE code = :'root_code'),
     (SELECT id FROM org.unit_type WHERE label = 'Region'),
     NULL, '5eed0000-0000-4000-a000-000000000202');
INSERT INTO org.unit (label, code, parent, unit_type, timezone, uuid) VALUES
    ('West Region', 'WRG', (SELECT id FROM org.unit WHERE code = :'root_code'),
     (SELECT id FROM org.unit_type WHERE label = 'Region'),
     NULL, '5eed0000-0000-4000-a000-000000000203');
INSERT INTO org.unit (label, code, parent, unit_type, timezone, uuid) VALUES
    ('Main Street Branch', 'MAIN', (SELECT id FROM org.unit WHERE code = 'ERG'),
     (SELECT id FROM org.unit_type WHERE label = 'Branch'),
     'America/Los_Angeles', '5eed0000-0000-4000-a000-000000000204');
INSERT INTO org.unit (label, code, parent, unit_type, timezone, uuid) VALUES
    ('Riverside Branch', 'RIVR', (SELECT id FROM org.unit WHERE code = 'ERG'),
     (SELECT id FROM org.unit_type WHERE label = 'Branch'),
     'America/Los_Angeles', '5eed0000-0000-4000-a000-000000000205');
INSERT INTO org.unit (label, code, parent, unit_type, timezone, uuid) VALUES
    ('Hilltop Branch', 'HILL', (SELECT id FROM org.unit WHERE code = 'WRG'),
     (SELECT id FROM org.unit_type WHERE label = 'Branch'),
     'America/Los_Angeles', '5eed0000-0000-4000-a000-000000000206');
INSERT INTO org.unit (label, code, parent, unit_type, timezone, uuid) VALUES
    ('Lakeside Branch', 'LAKE', (SELECT id FROM org.unit WHERE code = 'WRG'),
     (SELECT id FROM org.unit_type WHERE label = 'Branch'),
     'America/Los_Angeles', '5eed0000-0000-4000-a000-000000000207');
INSERT INTO org.unit (label, code, parent, unit_type, timezone, uuid) VALUES
    ('Main Street Locker', 'MAINL', (SELECT id FROM org.unit WHERE code = 'MAIN'),
     (SELECT id FROM org.unit_type WHERE label = 'Locker'),
     'America/Los_Angeles', '5eed0000-0000-4000-a000-000000000208');

COMMIT;
