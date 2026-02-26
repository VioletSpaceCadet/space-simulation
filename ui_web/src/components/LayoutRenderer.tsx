import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';

import type { GroupNode, LayoutNode, PanelId } from '../layout';

import { DraggableTab } from './DraggableTab';
import { DropZoneOverlay } from './DropZoneOverlay';

interface Props {
  layout: GroupNode
  renderPanel: (id: PanelId) => React.ReactNode
  isDragging: boolean
  activeDragId: PanelId | null
}

type SharedDragProps = {
  isDragging: boolean
  activeDragId: PanelId | null
  rootLayout: GroupNode
}

function RenderNode({
  node,
  renderPanel,
  ...drag
}: {
  node: LayoutNode
  renderPanel: Props['renderPanel']
} & SharedDragProps) {
  if (node.type === 'leaf') {
    return (
      <section className="relative flex flex-col h-full overflow-hidden bg-void p-3">
        <DraggableTab panelId={node.panelId} isDragging={drag.activeDragId === node.panelId} />
        <div className="flex-1 overflow-hidden mt-2">{renderPanel(node.panelId)}</div>
        <DropZoneOverlay
          panelId={node.panelId}
          active={drag.isDragging && drag.activeDragId !== node.panelId}
          layout={drag.rootLayout}
          dragSourceId={drag.activeDragId}
        />
      </section>
    );
  }

  return (
    <RenderGroup
      group={node}
      renderPanel={renderPanel}
      {...drag}
    />
  );
}

function RenderGroup({
  group,
  renderPanel,
  ...drag
}: {
  group: GroupNode
  renderPanel: Props['renderPanel']
} & SharedDragProps) {
  const defaultSize = 100 / group.children.length;

  return (
    <PanelGroup direction={group.direction}>
      {group.children.map((child, index) => {
        const key = child.type === 'leaf' ? child.panelId : `group-${index}`;
        const handleClass =
          group.direction === 'horizontal'
            ? 'w-px bg-edge hover:bg-dim cursor-col-resize transition-colors'
            : 'h-px bg-edge hover:bg-dim cursor-row-resize transition-colors';

        return (
          <span key={key} className="contents">
            {index > 0 && <PanelResizeHandle className={handleClass} />}
            <Panel defaultSize={defaultSize} minSize={10}>
              <RenderNode
                node={child}
                renderPanel={renderPanel}
                {...drag}
              />
            </Panel>
          </span>
        );
      })}
    </PanelGroup>
  );
}

export function LayoutRenderer({ layout, renderPanel, isDragging, activeDragId }: Props) {
  return (
    <RenderGroup
      group={layout}
      renderPanel={renderPanel}
      isDragging={isDragging}
      activeDragId={activeDragId}
      rootLayout={layout}
    />
  );
}
