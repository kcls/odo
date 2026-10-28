import type { TFunction } from "i18next"
import type { UIMatch } from "react-router"

/**
 * Route `handle` contract. A route that contributes a breadcrumb names
 * it from its own match (loader data, params) and the translator.
 */
export type RouteHandle = {
  crumb?: (match: UIMatch<unknown, RouteHandle>, t: TFunction) => string
}
