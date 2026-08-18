import { Injectable, inject } from '@angular/core';
import { MatSnackBar } from '@angular/material/snack-bar';

import { ApiRequestError } from './api';

/**
 * Backend error codes shared across tools that warrant a friendlier message
 * than the raw server text. Tool-specific codes are handled inline where the
 * context makes a better message possible; this covers the general cases and
 * anything not caught inline.
 */
const CODE_MESSAGES: Record<string, string> = {
  // Accounts / SAML
  NOT_LOCAL_ACCOUNT: $localize`This account is managed by the identity provider and can't be edited here.`,
  SAML_MANAGED: $localize`This item is managed by SAML and can't be changed here.`,
  // Referential conflicts (deletes blocked by dependents)
  PERMISSION_IN_USE: $localize`This permission is granted to one or more roles; revoke the grants first.`,
  ROLE_ASSIGNED: $localize`This role is assigned to users; remove the assignments first.`,
  ROLE_SAML_MAPPED: $localize`This role is referenced by SAML attribute mappings; remove those first.`,
  ATTRIBUTE_IN_USE: $localize`Role mappings reference this attribute; remove them first.`,
  ATTRIBUTE_HAS_VALUES: $localize`Captured user attribute values reference this attribute.`,
  IDP_IN_USE: $localize`This identity provider is referenced by service providers or sessions.`,
  IDP_HAS_IDENTITIES: $localize`Users have SAML identities from this IdP; it can't be deleted.`,
  SP_IN_USE: $localize`This service provider is referenced by pending authentication requests.`,
  TYPE_IN_USE: $localize`Active org units use this type; change them first.`,
  TYPE_HAS_CHILDREN: $localize`Other unit types list this type as their parent.`,
  UNIT_HAS_CHILDREN: $localize`This unit has active child units; move or delete them first.`,
  UNIT_IS_ROOT: $localize`The root org unit cannot be deleted.`,
  // Duplicate keys (usually shown inline, but safe as a fallback)
  PERMISSION_CODE_TAKEN: $localize`A permission with this code already exists.`,
  ROLE_CODE_TAKEN: $localize`A role with this code already exists.`,
  PERMISSION_ALREADY_GRANTED: $localize`This role already has that permission.`,
  ALREADY_ASSIGNED: $localize`The user already has this role at this org unit.`,
  ENTITY_ID_TAKEN: $localize`A config with this entity ID already exists.`,
  ATTRIBUTE_EXISTS: $localize`This IdP already tracks this attribute key with the same normalizer.`,
  MAPPING_EXISTS: $localize`This attribute value is already mapped to this role.`,
  TEMPLATE_CODE_TAKEN: $localize`A template with this code already exists.`,
  EMAIL_GROUP_CODE_TAKEN: $localize`An email group with this code already exists.`,
  EMAIL_ALREADY_IN_GROUP: $localize`This email address is already a member of the group.`,
  UNIT_CODE_TAKEN: $localize`An org unit with this code already exists.`,
  UNIT_LABEL_TAKEN: $localize`An org unit with this label already exists.`,
  TYPE_LABEL_TAKEN: $localize`A unit type with this label already exists.`,
};

/**
 * Fallback messages by HTTP status when the error has no known code and no
 * server-supplied message.
 */
function statusMessage(status: number): string {
  switch (status) {
    case 401:
      return $localize`Your session has expired. Please sign in again.`;
    case 403:
      return $localize`You don't have permission to do that.`;
    case 404:
      return $localize`That item no longer exists. It may have been changed by someone else.`;
    case 409:
      return $localize`That change conflicts with the current state. Refresh and try again.`;
    case 400:
      return $localize`The request was invalid. Check your input and try again.`;
    default:
      return status >= 500
        ? $localize`Something went wrong on the server. Please try again.`
        : $localize`The request could not be completed.`;
  }
}

/**
 * Turn any thrown value into a user-facing message. Prefers a known-code
 * message, then the server-supplied message, then a status-based fallback.
 */
export function describeError(err: unknown): string {
  if (err instanceof ApiRequestError) {
    if (err.code && CODE_MESSAGES[err.code]) return CODE_MESSAGES[err.code];
    if (err.message) return err.message;
    return statusMessage(err.status);
  }
  if (err instanceof Error && err.message) return err.message;
  return $localize`An unexpected error occurred.`;
}

/**
 * Central place to surface backend errors as a consistent, error-styled
 * snackbar. Use for any caught error a component doesn't handle inline.
 */
@Injectable({ providedIn: 'root' })
export class ErrorHandlerService {
  private readonly snackBar = inject(MatSnackBar);

  /** Log for diagnostics and show the user a friendly message. */
  show(err: unknown, context?: string): void {
    console.error(context ?? 'Request failed:', err);
    this.snackBar.open(describeError(err), $localize`:Snackbar dismiss action:Dismiss`, {
      duration: 6000,
      panelClass: 'app-error-snackbar',
    });
  }
}
