import { ErrorStateMatcher } from '@angular/material/core';

/**
 * Forces a mat-form-field into its error state whenever a server-supplied
 * field error is present, so the associated `mat-error` renders.
 *
 * Material only shows `<mat-error>` when the field's control is invalid and
 * touched. Our template-driven dialog inputs have no failing validator when
 * the server rejects a value (e.g. a duplicate code), so without this the
 * error message stays hidden. Bind one matcher per field:
 *
 *   protected readonly codeMatcher = new ServerErrorStateMatcher(this.codeError);
 *   <input matInput [errorStateMatcher]="codeMatcher" ...>
 */
export class ServerErrorStateMatcher implements ErrorStateMatcher {
  constructor(private readonly hasError: () => string) {}

  isErrorState(): boolean {
    return !!this.hasError();
  }
}
