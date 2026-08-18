import { Component, computed, inject, input } from '@angular/core';
import { MatIconModule } from '@angular/material/icon';
import { OdoTable } from './odo-table';

/**
 * Clickable, sortable table header. Place inside a `cdk-header-cell` and give
 * it the column's sort `key`; it reads the parent `<odo-table>`'s sort state to
 * render an asc/desc/neutral caret and calls back into the table on click, so
 * the host only wires `sort`/`sortChange` once on `<odo-table>`.
 *
 * ```html
 * <th cdk-header-cell *cdkHeaderCellDef>
 *   <odo-sort-header key="code">Code</odo-sort-header>
 * </th>
 * ```
 */
@Component({
  selector: 'odo-sort-header',
  imports: [MatIconModule],
  template: `
    <button type="button" class="odo-sort-header" (click)="toggle()">
      <ng-content />
      <mat-icon class="odo-sort-arrow" [class.odo-sort-arrow--active]="active()">
        {{ arrow() }}
      </mat-icon>
    </button>
  `,
  styleUrl: './odo-sort-header.scss',
})
export class OdoSortHeader {
  /** The column key this header sorts by (matches the API allow-list). */
  readonly key = input.required<string>();

  // The enclosing table — always present since this only lives inside one.
  private readonly table = inject(OdoTable);

  protected readonly active = computed(
    () => this.table.sort()?.active === this.key(),
  );

  protected readonly arrow = computed(() => {
    const sort = this.table.sort();
    if (sort?.active !== this.key()) return 'unfold_more';
    return sort.direction === 'asc' ? 'arrow_upward' : 'arrow_downward';
  });

  protected toggle(): void {
    this.table.toggleSort(this.key());
  }
}
