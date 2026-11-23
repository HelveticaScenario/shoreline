import {
    flexRender,
    getCoreRowModel,
    getSortedRowModel,
    type OnChangeFn,
    type SortingState,
    useReactTable,
} from '@tanstack/react-table';
import type { DisplayRow, ProfileColumns } from './columns';

const NUMERIC_COLUMNS = new Set(['self', 'params', 'total']);

const numericClass = (columnId: string) =>
    NUMERIC_COLUMNS.has(columnId) ? 'numeric' : '';

interface ModuleProfileTableProps {
    /** Keep `data` and `columns` stable between changes; the table re-sorts when either is replaced. */
    data: DisplayRow[];
    columns: ProfileColumns;
    sorting: SortingState;
    onSortingChange: OnChangeFn<SortingState>;
}

/**
 * The sortable table of per-module timings. `useReactTable` returns a mutable
 * instance whose methods read fresh state on every render, so React Compiler
 * must not memoize anything derived from it.
 */
export function ModuleProfileTable({
    data,
    columns,
    sorting,
    onSortingChange,
}: ModuleProfileTableProps) {
    'use no memo';
    // oxlint-disable-next-line react-hooks-js/incompatible-library
    const table = useReactTable({
        data,
        columns,
        state: { sorting },
        onSortingChange,
        getCoreRowModel: getCoreRowModel(),
        getSortedRowModel: getSortedRowModel(),
    });

    return (
        <table className="module-profile-table">
            <colgroup>
                <col className="col-module" />
                <col className="col-mode" />
                <col className="col-self" />
                <col className="col-params" />
                <col className="col-total" />
            </colgroup>
            <thead>
                {table.getHeaderGroups().map((hg) => (
                    <tr key={hg.id}>
                        {hg.headers.map((header) => {
                            const sortDir = header.column.getIsSorted();
                            return (
                                <th
                                    key={header.id}
                                    onClick={header.column.getToggleSortingHandler()}
                                    className={`${numericClass(header.column.id)} ${sortDir ? 'active' : ''}`}
                                >
                                    {flexRender(
                                        header.column.columnDef.header,
                                        header.getContext(),
                                    )}
                                    {sortDir === 'asc' && ' ▲'}
                                    {sortDir === 'desc' && ' ▼'}
                                </th>
                            );
                        })}
                    </tr>
                ))}
            </thead>
            <tbody>
                {table.getRowModel().rows.map((row) => (
                    <tr key={row.id}>
                        {row.getVisibleCells().map((cell) => (
                            <td
                                key={cell.id}
                                className={numericClass(cell.column.id)}
                            >
                                {flexRender(
                                    cell.column.columnDef.cell,
                                    cell.getContext(),
                                )}
                            </td>
                        ))}
                    </tr>
                ))}
            </tbody>
        </table>
    );
}
