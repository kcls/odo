import i18n from "i18next"
import LanguageDetector from "i18next-browser-languagedetector"
import resourcesToBackend from "i18next-resources-to-backend"
import { initReactI18next } from "react-i18next"

import { pseudoResource } from "./pseudo"

// Gotcha: interpolation variables share an object with t() options, so
// names like `lng`, `ns`, and `context` are not values. t("k", { ns })
// looks k up in namespace ns and renders the raw key. Use `language`
// and `namespace` instead.

/** Languages the app's own interface is available in. */
export const UI_LANGUAGES = ["en", "ar-XB"] as const
export type UiLanguage = (typeof UI_LANGUAGES)[number]

export const PSEUDO_LOCALES: readonly string[] = ["ar-XB"]

// Each namespace is its own chunk, fetched the first time a route needs
// it.
const bundles = import.meta.glob<{ default: Record<string, unknown> }>(
  "../locales/*/*.json"
)

async function loadBundle(lng: string, ns: string) {
  const isPseudo = PSEUDO_LOCALES.includes(lng)
  const load = bundles[`../locales/${isPseudo ? "en" : lng}/${ns}.json`]
  if (!load) throw new Error(`No ${ns} strings for ${lng}`)
  const { default: data } = await load()
  return isPseudo ? pseudoResource(data) : data
}

function applyDocumentLanguage(lng: string) {
  document.documentElement.lang = lng
  document.documentElement.dir = i18n.dir(lng)
}

i18n.on("languageChanged", applyDocumentLanguage)

export const i18nReady = i18n
  .use(resourcesToBackend(loadBundle))
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    supportedLngs: UI_LANGUAGES,
    fallbackLng: "en",
    load: "currentOnly",
    ns: ["common"],
    defaultNS: "common",
    interpolation: { escapeValue: false },
    detection: {
      order: ["localStorage", "navigator"],
      lookupLocalStorage: "odo-admin.lng",
      caches: ["localStorage"],
    },
  })

/**
 * Route loaders call this so a page's strings are ready before it
 * renders, instead of suspending mid-navigation.
 */
export function loadNamespaces(...ns: string[]): Promise<void> {
  return i18n.loadNamespaces(ns)
}

export default i18n
