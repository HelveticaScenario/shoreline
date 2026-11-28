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
    type RefObject,
} from 'react';
import {
    Tree,
    type DragPreviewProps,
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

/** What the drag preview needs from the tree that renders it. */
interface PreviewContext {
    tree: RefObject<TreeApi<FileTreeEntry> | null>;
    /** Whether a drag is over the tree. */
    dragInside: boolean;
}

const PreviewContext = createContext<PreviewContext | null>(null);

function EntryIcon({ node }: { node: NodeApi<FileTreeEntry> }) {
    if (node.isInternal) {
        return node.isOpen ? (
            <FolderOpen className="tree-icon" aria-hidden />
        ) : (
            <Folder className="tree-icon" aria-hidden />
        );
    }
    const FileIcon = FILE_ICONS[node.data.fileType ?? 'patch'];
    return <FileIcon className="tree-icon" aria-hidden />;
}

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

/**
 * The row height and indent the stylesheet sets, in pixels. They read as 0
 * while the explorer is hidden, and are measured again once it shows.
 */
function useRowMetrics() {
    const [probe, setProbe] = useState<HTMLDivElement | null>(null);
    const [metrics, setMetrics] = useState({ indent: 0, rowHeight: 0 });
    useLayoutEffect(() => {
        if (!probe) return;
        const measure = () =>
            setMetrics({
                indent: probe.offsetWidth,
                rowHeight: probe.offsetHeight,
            });
        measure();
        const observer = new ResizeObserver(measure);
        observer.observe(probe);
        return () => observer.disconnect();
    }, [probe]);
    return [setProbe, metrics] as const;
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
    const [dragInside, setDragInside] = useState(false);

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
            onDragEnter={() => setDragInside(true)}
            onDragOver={(e) => {
                // An entry dragged within the tree only moves, so a drag
                // held with Option or Alt, which asks to copy, moves too.
                if (
                    e.dataTransfer.dropEffect === 'copy' &&
                    treeRef.current?.dragNodes.length
                ) {
                    e.dataTransfer.dropEffect = 'move';
                }
            }}
            onDragLeave={(e) => {
                if (!e.currentTarget.contains(e.relatedTarget as Node | null)) {
                    setDragInside(false);
                }
            }}
            onDragEnd={() => setDragInside(false)}
            onDrop={() => setDragInside(false)}
        >
            <div className="workspace-tree-probe" ref={probeRef} aria-hidden />
            {container && rowHeight > 0 && (
                <PreviewContext.Provider value={{ dragInside, tree: treeRef }}>
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
                            disableDrag={(entry) =>
                                toAbsoluteWorkspacePath(
                                    ctx.workspaceRoot,
                                    entry.path,
                                ) === ctx.renamingPath
                            }
                            disableDrop={({ parentNode, dragNodes }) =>
                                dragNodes.every(
                                    (n) => n.parent?.id === parentNode.id,
                                )
                            }
                            onActivate={(node) => openNode(node, ctx, true)}
                            onToggle={() => {
                                const tree = treeRef.current;
                                if (tree) {
                                    storeExpandedFolders(
                                        tree.openState,
                                        fileTree,
                                    );
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
                            renderDragPreview={DragPreview}
                            width={width}
                            height={height}
                            rowHeight={rowHeight}
                            // A screenful of rows past each edge, so a fast
                            // scroll never shows blank space before they render.
                            overscanCount={Math.ceil(height / rowHeight)}
                            indent={indent}
                            className="workspace-tree-list"
                            rowClassName="workspace-tree-row"
                            aria-label="Workspace files"
                        >
                            {TreeRow}
                        </Tree>
                    </RowContext.Provider>
                </PreviewContext.Provider>
            )}
        </div>
    );
}

// A move always lands in a folder, whose rows highlight to show it.
const NoCursor = () => null;

/**
 * The row a patch file or folder drag carries, shown only over the tree since
 * such a drag can land nowhere else. An asset drag shows the browser's own
 * drag image, which follows the pointer into the editor.
 */
function DragPreview({ id, offset, isDragging }: DragPreviewProps) {
    const { dragInside, tree } = useContext(PreviewContext)!;
    const node = id ? tree.current?.get(id) : null;
    if (!isDragging || !dragInside || !offset || !node || isAsset(node.data)) {
        return null;
    }
    return (
        <div
            className="tree-entry tree-drag-preview"
            style={{ transform: `translate(${offset.x}px, ${offset.y}px)` }}
        >
            <EntryIcon node={node} />
            <span className="tree-name">{node.data.name}</span>
        </div>
    );
}

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

    // Only an asset may land in the editor, as a copy of its path. Any other
    // entry only moves, so the editor refuses it, and it keeps the tree's
    // empty drag image so no ghost of it follows the pointer there.
    const handleDragStart = (e: DragEvent<HTMLDivElement>) => {
        if (!asset) {
            e.dataTransfer.effectAllowed = 'move';
            return;
        }
        const rect = e.currentTarget.getBoundingClientRect();
        e.dataTransfer.setDragImage(
            e.currentTarget,
            e.clientX - rect.left,
            e.clientY - rect.top,
        );
        e.dataTransfer.setData(WORKSPACE_PATH_MIME, `/${entry.path}`);
        e.dataTransfer.effectAllowed = 'copyMove';
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
            onDoubleClick={
                node.isLeaf ? () => openNode(node, ctx, false) : undefined
            }
            onContextMenu={(e) => ctx.onContextMenu(e, entry)}
            onDragStart={handleDragStart}
        >
            <EntryIcon node={node} />
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
