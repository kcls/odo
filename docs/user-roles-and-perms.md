# User Roles and Permissions

## Basics

Access to perform actions is granted through a combination of roles and permissions.

### Roles

Roles are assigned to users at org units. The assigned role defines
which permissions are applicable. The assigned org unit defines the
portion of the org unit tree where the role is active: a role assigned
to an org unit implicitly also assigns the role at all descendant/child org units.

Note that "granting a role at an org unit" and "having permission at an org unit" 
are not identical concepts.  More below.

### Permissions

Permissions define an action to be performed and a minimum depth (maximum
height in the org unit tree) where the permission applies.

### Roles + Permissions

The combination of a role org unit and a permission's minimum depth define
total range of permissibility.

### Three Examples

#### An Org Tree

* Root (depth 0)
  * Region-A (depth 1)
    * Branch-A1 (depth 2)
    * Branch-A2 (depth 2)
      * Department-A2 (depth 3)
  * Region-B (depth 1)
    * Branch-B1 (depth 2)
    * Branch-B2 (depth 2)

#### Example 1: Role Depth Matches Permission Depth

* User U has the 'reporter' role at Branch-A2. The role grants the
  'report.create' permission with `min_depth` of 2.
* U can create reports at Branch-A2 and Department-A2.

#### Example 2: Role Depth Deeper Than Permission Depth

* User U has the 'reporter' role at Branch-A2. The role grants the
  'report.create' permission with `min_depth` of 1.
* U can create reports at Region-A, Branch-A1, Branch-A2, and Department-A2.

In this scenario, the permission checker walks up the org unit tree
until it encounters the org unit at the permission depth (`min_depth`
1 = Region-A), then includes all descendant org units at depth >=
`min_depth`. This enables "sibling expansion" - Branch-A1 is included
even though it's a sibling of Branch-A2.

#### Example 3: Role Depth Shallower Than Permission Depth

* User U has the 'reporter' role at Region-A. The role grants the
  'report.create' permission with `min_depth` of 2.
* U can create reports at Branch-A1, Branch-A2, and Department-A2.
  * Notably U cannot create reports linked to Region-A since the permission
    `min_depth` forbids it.

In this scenario, the permission checker grants the permission to U at
all descendant org units of the role org unit that have depth >= `min_depth` (depth >= 2).

## Database Schema

User roles and permissions are primarily expressed through 4 DB tables.

* ```authz.permission```
  * Specifies an action a user is allowed to perform.
* ```authz.role```
  * Defines a collection of permissions.
* ```authz.role_permission```
  * Links a permission to a role and specifies the minimum org unit tree depth 
    where the permission is granted.
* ```authz.usr_role_org_map```
  * Links a user to a role at an org unit, including its descendant org units.

