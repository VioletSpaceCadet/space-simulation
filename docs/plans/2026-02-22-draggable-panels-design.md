# Draggable Panel Layout Design

## Goal

Make frontend panels draggable and reorderable with VS Code-style vertical stacking support.

## Architecture

Replace the flat panel array with a **layout tree**. Each node is either a leaf (single panel) or a group (horizontal/vertical split containing children).

```ts
type LayoutNode =
  | { type: "leaf"; panelId: PanelId }
  | { type: "group"; direction: "horizontal" | "vertical"; children: LayoutNode[] };
```

Root is always a horizontal group. Vertical stacking creates nested vertical groups.

## Interaction Model

**Tab bar:** Every panel gets a draggable header tab. Drag tabs left/right to reorder panels at the same level.

**Edge drop zones:** When dragging, translucent overlays appear on top/bottom/left/right edges of other panels. Top/bottom = vertical stack. Left/right = insert before/after.

**Animation:** `@dnd-kit` CSS transform-based drag with semi-transparent preview. Drop zones highlight with accent color overlay.

## State Management

- `useLayoutState` hook manages the layout tree
- Serialized to localStorage on every change
- Panel visibility toggles add/remove leaf nodes from the tree
- Default: all visible panels in a single horizontal row

## Component Structure

```
App.tsx
  StatusBar
  Sidebar (toggle buttons)
  DndContext (@dnd-kit)
    LayoutRenderer (recursive)
      PanelGroup direction="horizontal"
        Panel + DropZoneOverlay
          DraggableTab (panel header)
          PanelContent (Map/Events/etc.)
        PanelResizeHandle
        Panel + DropZoneOverlay
          PanelGroup direction="vertical" (if stacked)
            Panel + DraggableTab + Content
            PanelResizeHandle
            Panel + DraggableTab + Content
```

## New Files

| File | Purpose |
|---|---|
| `useLayoutState.ts` | Layout tree state, localStorage persistence, tree manipulation |
| `LayoutRenderer.tsx` | Recursive component rendering layout tree with PanelGroup/Panel |
| `DraggableTab.tsx` | Draggable panel header tab |
| `DropZoneOverlay.tsx` | Edge drop zone indicators during drag |

## Modified Files

| File | Change |
|---|---|
| `App.tsx` | Wrap in DndContext, replace flat panel rendering with LayoutRenderer |

## Dependencies

- `@dnd-kit/core` — drag context, sensors, collision detection
- `@dnd-kit/sortable` — sortable list for tab reorder
- `@dnd-kit/utilities` — CSS transform utilities

## Persistence

Layout tree JSON saved to localStorage key `panel-layout`. Loaded on mount with fallback to default horizontal layout.
