import {
    AudioLines,
    Film,
    Folder,
    FolderOpen,
    Image,
    KeyboardMusic,
    Parentheses,
    type LucideIcon,
} from 'lucide-react';
import {
    createContext,
    useContext,
    useLayoutEffect,
    useRef,
    useState,
    type DragEvent,
    type KeyboardEvent,
    type MouseEvent,
} from 'react';
import {
    Tree,
    type NodeApi,
    type NodeRendererProps,
    type TreeApi,
} from 'react-arborist';
import type { FileTreeEntry } from '../../../shared/ipcTypes';
import { toAbsoluteWorkspacePath } from '../../app/buffers';
import { WORKSPACE_PATH_MIME } from '../monaco/workspacePathCompletion';
import { readExpandedFolders, storeExpandedFolders } from './expandedFolders';
import { RenameInput } from './RenameInput';

const FILE_ICONS: Record<NonNullable<FileTreeEntry['fileType']>, LucideIcon> = {
    patch: Parentheses,
    wav: AudioLines,
    image: Image,
    video: Film,
    syx: KeyboardMusic,
};

interface WorkspaceTreeProps {
    workspaceRoot: string;
    fileTree: FileTreeEntry[];
    /** Absolute path of the file shown in the editor. */
    activeFilePath?: string;
    /** Absolute path of the entry being renamed. */
    renamingPath: string | null;
    onOpenFile: (relPath: string, options?: { preview?: boolean }) => void;
    onContextMenu: (e: MouseEvent, entry: FileTreeEntry) => void;
    onRenameCommit: (path: string, newName: string) => void;
    onRenameCancel: () => void;
    /** Moves a workspace-relative entry to another workspace-relative path. */
    onMove: (fromPath: string, toPath: string) => void;
}

type RowContext = Omit<WorkspaceTreeProps, 'fileTree' | 'onMove'>;

const RowContext = createContext<RowContext | null>(null);

const isAsset = (entry: FileTreeEntry) =>
    entry.fileType !== undefined && entry.fileType !== 'patch';

const childrenOf = (entry: FileTreeEntry) =>
    entry.type === 'directory' ? (entry.children ?? []) : null;

/** Opens a patch file, or expands or collapses a folder. */
function openNode(
    node: NodeApi<FileTreeEntry>,
    ctx: RowContext,
    preview: boolean,
) {
    const entry = node.data;
    const absolutePath = toAbsoluteWorkspacePath(ctx.workspaceRoot, entry.path);
    if (ctx.renamingPath === absolutePath) return;
    if (node.isInternal) {
        node.toggle();
    } else if (!isAsset(entry)) {
        ctx.onOpenFile(entry.path, { preview });
    }
}

/** The row height and indent the stylesheet sets, in pixels. */
function useRowMetrics() {
    const probeRef = useRef<HTMLDivElement>(null);
    const [metrics, setMetrics] = useState({ indent: 0, rowHeight: 0 });
    useLayoutEffect(() => {
        const probe = probeRef.current;
        if (!probe) return;
        setMetrics({
            indent: probe.offsetWidth,
            rowHeight: probe.offsetHeight,
        });
    }, []);
    return [probeRef, metrics] as const;
}

/** The content-box size of the element `ref` is attached to. */
function useElementSize() {
    const [element, setElement] = useState<HTMLDivElement | null>(null);
    const [size, setSize] = useState({ height: 0, width: 0 });
    useLayoutEffect(() => {
        if (!element) return;
        const measure = () =>
            setSize({
                height: element.clientHeight,
                width: element.clientWidth,
            });
        measure();
        const observer = new ResizeObserver(measure);
        observer.observe(element);
        return () => observer.disconnect();
    }, [element]);
    return [setElement, element, size] as const;
}

/**
 * The workspace's files and folders. A click opens a patch as a preview, a
 * double click or Enter keeps it open, and dragging an entry onto a folder
 * moves it there. Asset files dragged into the editor insert their path.
 */
export function WorkspaceTree({
    fileTree,
    onMove,
    ...ctx
}: WorkspaceTreeProps) {
    const treeRef = useRef<TreeApi<FileTreeEntry> | null>(null);
    const [initialOpenState] = useState(readExpandedFolders);
    const [probeRef, { indent, rowHeight }] = useRowMetrics();
    const [setContainer, container, { width, height }] = useElementSize();

    const activeId =
        ctx.activeFilePath?.startsWith(`${ctx.workspaceRoot}/`) === true
            ? ctx.activeFilePath.slice(ctx.workspaceRoot.length + 1)
            : undefined;

    const handleKeyDown = (e: KeyboardEvent) => {
        // The tree's own key handling leaves Enter alone while no rename
        // handler is set; renames run through the explorer's context menu.
        if (e.key !== 'Enter' || (e.target as Element).closest('input')) {
            return;
        }
        const node = treeRef.current?.focusedNode;
        if (node) {
            e.preventDefault();
            openNode(node, ctx, false);
        }
    };

    return (
        <div
            className="workspace-tree"
            ref={setContainer}
            onKeyDown={handleKeyDown}
        >
            <div className="workspace-tree-probe" ref={probeRef} aria-hidden />
            {container && rowHeight > 0 && (
                <RowContext.Provider value={ctx}>
                    <Tree<FileTreeEntry>
                        ref={treeRef}
                        data={fileTree}
                        idAccessor="path"
                        childrenAccessor={childrenOf}
                        openByDefault={false}
                        initialOpenState={initialOpenState}
                        selection={activeId}
                        disableMultiSelection
                        disableEdit
                        disableDrop={({ parentNode, dragNodes }) =>
                            dragNodes.every(
                                (n) => n.parent?.id === parentNode.id,
                            )
                        }
                        onActivate={(node) => openNode(node, ctx, true)}
                        onToggle={() => {
                            const tree = treeRef.current;
                            if (tree) {
                                storeExpandedFolders(tree.openState, fileTree);
                            }
                        }}
                        onMove={({ dragNodes, parentNode }) => {
                            const dir =
                                parentNode && !parentNode.isRoot
                                    ? `${parentNode.data.path}/`
                                    : '';
                            for (const node of dragNodes) {
                                onMove(
                                    node.data.path,
                                    `${dir}${node.data.name}`,
                                );
                            }
                        }}
                        dndRootElement={container}
                        renderCursor={NoCursor}
                        renderDragPreview={NoDragPreview}
                        width={width}
                        height={height}
                        rowHeight={rowHeight}
                        indent={indent}
                        className="workspace-tree-list"
                        rowClassName="workspace-tree-row"
                        aria-label="Workspace files"
                    >
                        {TreeRow}
                    </Tree>
                </RowContext.Provider>
            )}
        </div>
    );
}

// Moves always land in a folder, whose rows highlight instead of a line
// between rows, and the browser's own drag image follows the pointer out
// of the tree into the editor.
const NoCursor = () => null;
const NoDragPreview = () => null;

function TreeRow({
    node,
    tree,
    style,
    dragHandle,
}: NodeRendererProps<FileTreeEntry>) {
    const ctx = useContext(RowContext)!;
    const entry = node.data;
    const absolutePath = toAbsoluteWorkspacePath(ctx.workspaceRoot, entry.path);
    const isRenaming = ctx.renamingPath === absolutePath;
    const asset = isAsset(entry);

    const destination = tree.dragDestinationParent;
    const inDropTarget =
        destination !== null &&
        !destination.isRoot &&
        (destination.id === node.id || destination.isAncestorOf(node));

    const Icon = node.isInternal
        ? node.isOpen
            ? FolderOpen
            : Folder
        : FILE_ICONS[entry.fileType ?? 'patch'];

    const className = [
        'tree-entry',
        node.isInternal ? 'tree-folder' : 'tree-file',
        asset && 'tree-file-asset',
        absolutePath === ctx.activeFilePath && 'active',
        node.isFocused && 'focused',
        node.isDragging && 'dragging',
        inDropTarget && 'drop-target',
    ]
        .filter(Boolean)
        .join(' ');

    const handleDragStart = (e: DragEvent<HTMLDivElement>) => {
        const rect = e.currentTarget.getBoundingClientRect();
        e.dataTransfer.setDragImage(
            e.currentTarget,
            e.clientX - rect.left,
            e.clientY - rect.top,
        );
        if (asset) {
            e.dataTransfer.setData(WORKSPACE_PATH_MIME, `/${entry.path}`);
            e.dataTransfer.effectAllowed = 'copyMove';
        }
    };

    return (
        <div
            ref={dragHandle}
            style={{
                paddingLeft: `calc(var(--sp-1) + ${style.paddingLeft}px)`,
            }}
            className={className}
            title={
                asset ? `Drag into the editor to use /${entry.path}` : undefined
            }
            onDoubleClick={() => openNode(node, ctx, false)}
            onContextMenu={(e) => !asset && ctx.onContextMenu(e, entry)}
            onDragStart={handleDragStart}
        >
            <Icon className="tree-icon" aria-hidden />
            {isRenaming ? (
                <RenameInput
                    name={entry.name}
                    onCommit={(newName) =>
                        ctx.onRenameCommit(absolutePath, newName)
                    }
                    onCancel={ctx.onRenameCancel}
                    onClick={(e) => e.stopPropagation()}
                />
            ) : (
                <span className="tree-name">{entry.name}</span>
            )}
        </div>
    );
}
