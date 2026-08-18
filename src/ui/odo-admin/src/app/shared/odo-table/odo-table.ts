import {
  Component,
  ViewEncapsulation,
  contentChildren,
  effect,
  input,
  output,
  viewChild,
} from '@angular/core';
import { CdkColumnDef, CdkTable, CdkTableModule } from '@angular/cdk/table';

/** Active sort state for a server-driven list. `null` = no explicit sort. */
export interface OdoSort {
  /** The sortable column's key (matches an `<odo-sort-header key>`). */
  active: string;
  direction: 'asc' | 'desc';
}

/**
 * Styled data table built on CDK's `cdk-table` — deliberately NOT Material's
 * `mat-table`, so we own every pixel of the presentation (framing, density,
 * hover, dark/light) through our own CSS with zero `.mat-mdc-*` overrides.
 *
 * Usage mirrors mat-table: project column definitions and pass the column
 * order via `displayedColumns`.
 *
 * ```html
 * <odo-table [rows]="items()" [displayedColumns]="['code','actions']"
 *            [clickableRows]="true" (rowClick)="open($event)">
 *   <ng-container cdkColumnDef="code">
 *     <th cdk-header-cell *cdkHeaderCellDef>Code</th>
 *     <td cdk-cell *cdkCellDef="let row">{{ row.code }}</td>
 *   </ng-container>
 * </odo-table>
 * ```
 *
 * For server-driven sorting, bind `sort`/`sortChange` and wrap the sortable
 * headers in `<odo-sort-header key="...">`:
 *
 * ```html
 * <odo-table [rows]="rows()" [displayedColumns]="cols"
 *            [sort]="sort()" (sortChange)="onSort($event)">
 *   <ng-container cdkColumnDef="code">
 *     <th cdk-header-cell *cdkHeaderCellDef>
 *       <odo-sort-header key="code">Code</odo-sort-header>
 *     </th>
 *     <td cdk-cell *cdkCellDef="let row">{{ row.code }}</td>
 *   </ng-container>
 * </odo-table>
 * ```
 */
@Component({
  selector: 'odo-table',
  imports: [CdkTableModule],
  templateUrl: './odo-table.html',
  styleUrl: './odo-table.scss',
  // The column cells (<td cdk-cell>) are projected from the *host* component,
  // so with emulated encapsulation odo-table's scoping attribute never reaches
  // them and the cell styles don't apply. None makes this stylesheet apply to
  // the CDK table markup wherever it's projected; the `.odo-table` prefix on
  // every rule keeps it scoped to this component in practice.
  encapsulation: ViewEncapsulation.None,
})
export class OdoTable<T> {
  /** Rows to render. */
  readonly rows = input.required<readonly T[]>();
  /** Column ids, in display order. Must match projected cdkColumnDef names. */
  readonly displayedColumns = input.required<string[]>();
  /** When true, rows show a pointer cursor + hover and emit rowClick. */
  readonly clickableRows = input(false);
  /**
   * "plain" drops the outer frame/background — for tables nested inside a
   * card that already supplies a surface. "framed" (default) is the
   * standalone list style.
   */
  readonly variant = input<'framed' | 'plain'>('framed');

  /** Current sort state (server-driven). Read by projected sort headers. */
  readonly sort = input<OdoSort | null>(null);

  /** Emitted when a row is clicked (only when clickableRows is true). */
  readonly rowClick = output<T>();

  /** Emitted when a sortable header is clicked, with the next sort state. */
  readonly sortChange = output<OdoSort>();

  private readonly table = viewChild.required(CdkTable);
  private readonly columnDefs = contentChildren(CdkColumnDef);

  /**
   * Called by a projected `<odo-sort-header>` when clicked. Cycles the
   * direction for the active column (asc -> desc), or starts a new column at
   * asc, and emits the result for the host to re-fetch with.
   */
  toggleSort(key: string): void {
    const current = this.sort();
    const direction: 'asc' | 'desc' =
      current?.active === key && current.direction === 'asc' ? 'desc' : 'asc';
    this.sortChange.emit({ active: key, direction });
  }

  constructor() {
    // Column defs are projected via <ng-content>, so they aren't discovered
    // by the inner cdk-table automatically — register them explicitly.
    effect((onCleanup) => {
      const table = this.table();
      const defs = this.columnDefs();
      for (const def of defs) {
        table.addColumnDef(def);
      }
      onCleanup(() => {
        for (const def of defs) {
          table.removeColumnDef(def);
        }
      });
    });
  }
}
