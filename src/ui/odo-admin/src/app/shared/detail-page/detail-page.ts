import { Component, input } from '@angular/core';
import { RouterLink } from '@angular/router';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';

/**
 * Drill-down chrome for a subject detail page: a back link, the subject
 * title, a projected summary line, projected header actions, and a projected
 * body. The body is intentionally unopinionated — a subject composes stacked
 * sections, tabs, or a master/detail split inside it, whatever best conveys
 * "more about this subject". The chrome (back affordance + title + summary)
 * is what makes every detail page feel like a consistent drill-down.
 *
 * The summary line is a flex row: project each summary fact as its OWN
 * `slot="summary"` element so the row's gap spaces them. Do NOT wrap them in a
 * single `<div slot="summary">` — that becomes one flex item and the facts run
 * together with no spacing.
 *
 * ```html
 * <odo-detail-page [backLink]="'/org-units'" backLabel="Org Units"
 *                  i18n-backLabel [title]="unit().label">
 *   <span slot="summary" class="cell-code">{{ unit().code }}</span>
 *   <span slot="summary">{{ typeLabel() }}</span>
 *   <div slot="actions"><button mat-stroked-button>Edit</button></div>
 *   <!-- default slot: the body -->
 *   <section>...</section>
 * </odo-detail-page>
 * ```
 */
@Component({
  selector: 'odo-detail-page',
  imports: [RouterLink, MatButtonModule, MatIconModule],
  templateUrl: './detail-page.html',
  styleUrl: './detail-page.scss',
})
export class DetailPage {
  /** Router link for the back affordance (the parent list route). */
  readonly backLink = input.required<string>();
  /** Label for the back link (e.g. "Org Units"). */
  readonly backLabel = input.required<string>();
  /** Subject title shown as the page heading. */
  readonly title = input.required<string>();
}
