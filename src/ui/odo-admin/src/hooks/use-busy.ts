import { useLocation, useNavigation } from "react-router"

/**
 * True while the router is reloading the current page's data (a search,
 * sort, or page change, or a revalidation), so a list can dim stale rows
 * instead of blanking them.
 */
export function useBusy(): boolean {
  const navigation = useNavigation()
  const location = useLocation()
  return navigation.state === "loading" && navigation.location?.pathname === location.pathname
}
