import "i18next"

import type common from "@/locales/en/common.json"
import type home from "@/locales/en/home.json"
import type login from "@/locales/en/login.json"
import type orgUnits from "@/locales/en/orgUnits.json"
import type permissions from "@/locales/en/permissions.json"
import type roles from "@/locales/en/roles.json"
import type templates from "@/locales/en/templates.json"
import type unitTypes from "@/locales/en/unitTypes.json"
import type users from "@/locales/en/users.json"

// English is the source of truth for keys: a t() call with a key that
// is not in these files fails the typecheck.
declare module "i18next" {
  interface CustomTypeOptions {
    defaultNS: "common"
    resources: {
      common: typeof common
      home: typeof home
      login: typeof login
      orgUnits: typeof orgUnits
      permissions: typeof permissions
      roles: typeof roles
      templates: typeof templates
      unitTypes: typeof unitTypes
      users: typeof users
    }
  }
}
