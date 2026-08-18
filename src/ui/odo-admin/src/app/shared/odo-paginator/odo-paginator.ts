import {
  Component,
  computed,
  input,
  output,
} from '@angular/core';
import { MatIconModule } from '@angular/material/icon';

/**
 * Emitted when the page or page size changes. Shape-compatible with Angular
 * Material's `PageEvent`, so existing server-driven handlers (which read
 * `pageIndex` + `pageSize`) work unchanged.
 */
export interface OdoPageEvent {
  pageIndex: number;
  pageSize: number;
}

/**
 * Pagination control for server-driven lists, built on plain buttons + a
 * native `<select>` — deliberately NOT Material's `mat-paginator`, so we own
 * every pixel through our own CSS with zero `.mat-mdc-*` overrides. Pairs with
 * `<odo-table>`: place it directly beneath the table and drive the table's rows
 * from the server response for the emitted page.
 *
 * ```html
 * <odo-table [rows]="rows()" [displayedColumns]="cols" />
 * <odo-paginator
 *   [length]="total()"
 *   [pageIndex]="pageIndex()"
 *   [pageSize]="pageSize()"
 *   (page)="onPage($event)"
 * />
 * ```
 */
@Component({
  selector: 'odo-paginator',
  imports: [MatIconModule],
  templateUrl: './odo-paginator.html',
  styleUrl: './odo-paginator.scss',
})
export class OdoPaginator {
  /** Total number of rows matching the filter (across all pages). */
  readonly length = input.required<number>();
  /** Zero-based index of the current page. */
  readonly pageIndex = input<number>(0);
  /** Rows per page. Must be one of `pageSizeOptions`. */
  readonly pageSize = input<number>(25);
  /** Selectable page sizes. */
  readonly pageSizeOptions = input<number[]>([10, 25, 50, 100]);

  /** Emitted when the user changes page or page size. */
  readonly page = output<OdoPageEvent>();

  /** Total number of pages (at least 1). */
  protected readonly pageCount = computed(() =>
    Math.max(1, Math.ceil(this.length() / this.pageSize())),
  );

  /** 1-based first/last row shown, for the "X–Y of N" label. */
  protected readonly rangeStart = computed(() =>
    this.length() === 0 ? 0 : this.pageIndex() * this.pageSize() + 1,
  );
  protected readonly rangeEnd = computed(() =>
    Math.min(this.length(), (this.pageIndex() + 1) * this.pageSize()),
  );

  protected readonly canPrev = computed(() => this.pageIndex() > 0);
  protected readonly canNext = computed(
    () => this.pageIndex() < this.pageCount() - 1,
  );

  protected prev(): void {
    if (this.canPrev()) {
      this.page.emit({ pageIndex: this.pageIndex() - 1, pageSize: this.pageSize() });
    }
  }

  protected next(): void {
    if (this.canNext()) {
      this.page.emit({ pageIndex: this.pageIndex() + 1, pageSize: this.pageSize() });
    }
  }

  protected first(): void {
    if (this.canPrev()) {
      this.page.emit({ pageIndex: 0, pageSize: this.pageSize() });
    }
  }

  protected last(): void {
    if (this.canNext()) {
      this.page.emit({ pageIndex: this.pageCount() - 1, pageSize: this.pageSize() });
    }
  }

  /** Page size changed: reset to the first page (offset would otherwise skip rows). */
  protected onSizeChange(value: string): void {
    this.page.emit({ pageIndex: 0, pageSize: Number(value) });
  }
}
