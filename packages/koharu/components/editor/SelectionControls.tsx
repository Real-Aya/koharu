'use client'

import { useEffect, useRef, type PointerEvent as ReactPointerEvent, type RefObject } from 'react'

import {
  cssFrame,
  growFrame,
  nudgeFrame,
  pagePoint,
  resizeFrame,
  rotateFrame,
  type Camera,
  type ResizeHandle,
} from '@/lib/geometry'
import type { EntityId, Frame, Point, TransformFrame } from '@koharu/bridge/protocol'

interface SelectionControlsProps {
  container: RefObject<HTMLDivElement | null>
  element: EntityId
  frame: Frame
  camera: Camera
  edgesOnly: boolean
  /** When set, shows a button that re-runs automatic text fitting for the layer. */
  onRefit?: () => void
  refitLabel?: string
  onTransformStart: (elements: TransformFrame[]) => void
  onTransformFrame: (elements: TransformFrame[]) => void
  onTransformEnd: () => void
}

type ControlGesture =
  | { kind: 'resize'; pointer: number; original: Frame; handle: ResizeHandle }
  | { kind: 'rotate'; pointer: number; original: Frame; start: Point }

// Pointer targets are CSS pixels, so they stay the same size on screen at every zoom
// level. Corners get the largest target and sit above the edge strips that run between them.
const CORNER_HIT = 22
const EDGE_HIT = 12
const COARSE_POINTER_SCALE = 1.5

const handles: Array<{
  handle: ResizeHandle
  left: string
  top: string
  cursor: string
}> = [
  { handle: 'nw', left: '0%', top: '0%', cursor: 'nwse-resize' },
  { handle: 'n', left: '50%', top: '0%', cursor: 'ns-resize' },
  { handle: 'ne', left: '100%', top: '0%', cursor: 'nesw-resize' },
  { handle: 'e', left: '100%', top: '50%', cursor: 'ew-resize' },
  { handle: 'se', left: '100%', top: '100%', cursor: 'nwse-resize' },
  { handle: 's', left: '50%', top: '100%', cursor: 'ns-resize' },
  { handle: 'sw', left: '0%', top: '100%', cursor: 'nesw-resize' },
  { handle: 'w', left: '0%', top: '50%', cursor: 'ew-resize' },
]

const arrowDirections: Record<string, Point> = {
  ArrowLeft: { x: -1, y: 0 },
  ArrowRight: { x: 1, y: 0 },
  ArrowUp: { x: 0, y: -1 },
  ArrowDown: { x: 0, y: 1 },
}

function isEditableTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) return false
  return (
    target.isContentEditable ||
    target.closest('input, textarea, select, [contenteditable=""], [contenteditable="true"]') !==
      null
  )
}

export function SelectionControls({
  container,
  element,
  frame,
  camera,
  edgesOnly,
  onRefit,
  refitLabel = 'Refit text',
  onTransformStart,
  onTransformFrame,
  onTransformEnd,
}: SelectionControlsProps) {
  const gesture = useRef<ControlGesture | null>(null)
  const position = cssFrame(frame, camera)
  const coarse = typeof window !== 'undefined' && window.matchMedia?.('(pointer: coarse)').matches
  const scale = coarse ? COARSE_POINTER_SCALE : 1
  const cornerHit = CORNER_HIT * scale
  const edgeHit = EDGE_HIT * scale

  // Keyboard edits run as one transform gesture per key burst so undo stays coarse.
  const latest = useRef({ frame, element, camera })
  latest.current = { frame, element, camera }
  const keyboardFrame = useRef<Frame | null>(null)
  const callbacks = useRef({ onTransformStart, onTransformFrame, onTransformEnd })
  callbacks.current = { onTransformStart, onTransformFrame, onTransformEnd }

  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      const direction = arrowDirections[event.key]
      if (!direction || event.ctrlKey || event.metaKey || isEditableTarget(event.target)) return
      if (gesture.current) return
      event.preventDefault()
      const { frame: current, element: id, camera: view } = latest.current
      const step = event.shiftKey ? 10 : 1
      // One screen pixel in page units, so tiny nudges stay visible when zoomed out.
      const pixel = window.devicePixelRatio / view.zoom
      const amount = Math.max(step, step === 1 ? pixel : 0)
      if (!keyboardFrame.current) {
        keyboardFrame.current = current
        callbacks.current.onTransformStart([{ element: id, frame: current }])
      }
      const base = keyboardFrame.current
      // Alt+arrows resize around the centre instead of moving.
      const next = event.altKey
        ? growFrame(base, direction.x * amount, direction.y * amount, 1)
        : nudgeFrame(base, direction.x * amount, direction.y * amount)
      keyboardFrame.current = next
      callbacks.current.onTransformFrame([{ element: id, frame: next }])
    }
    const keyup = (event: KeyboardEvent) => {
      if (!(event.key in arrowDirections) || !keyboardFrame.current) return
      keyboardFrame.current = null
      callbacks.current.onTransformEnd()
    }
    window.addEventListener('keydown', keydown)
    window.addEventListener('keyup', keyup)
    return () => {
      window.removeEventListener('keydown', keydown)
      window.removeEventListener('keyup', keyup)
      if (keyboardFrame.current) {
        keyboardFrame.current = null
        callbacks.current.onTransformEnd()
      }
    }
  }, [])

  const eventPoint = (event: ReactPointerEvent<HTMLDivElement>): Point | null => {
    const bounds = container.current?.getBoundingClientRect()
    return bounds ? pagePoint(event.clientX, event.clientY, bounds, camera) : null
  }

  const capture = (event: ReactPointerEvent<HTMLDivElement>) => {
    event.preventDefault()
    event.stopPropagation()
    event.currentTarget.setPointerCapture(event.pointerId)
  }

  const startResize = (handle: ResizeHandle) => (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || gesture.current) return
    capture(event)
    gesture.current = { kind: 'resize', pointer: event.pointerId, original: frame, handle }
    onTransformStart([{ element, frame }])
  }

  const startRotate = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || gesture.current) return
    const start = eventPoint(event)
    if (!start) return
    capture(event)
    gesture.current = { kind: 'rotate', pointer: event.pointerId, original: frame, start }
    onTransformStart([{ element, frame }])
  }

  const update = (event: ReactPointerEvent<HTMLDivElement>) => {
    const current = gesture.current
    if (!current || current.pointer !== event.pointerId) return
    event.preventDefault()
    event.stopPropagation()
    const point = eventPoint(event)
    if (!point) return
    const next =
      current.kind === 'resize'
        ? resizeFrame(
            current.original,
            current.handle,
            point,
            window.devicePixelRatio / camera.zoom,
            { keepAspect: event.shiftKey, fromCenter: event.altKey },
          )
        : rotateFrame(current.original, current.start, point)
    onTransformFrame([{ element, frame: next }])
  }

  const finish = (event: ReactPointerEvent<HTMLDivElement>) => {
    const current = gesture.current
    if (!current || current.pointer !== event.pointerId) return
    event.preventDefault()
    event.stopPropagation()
    gesture.current = null
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId)
    }
    onTransformEnd()
  }

  const lostCapture = (event: ReactPointerEvent<HTMLDivElement>) => {
    const current = gesture.current
    if (!current || current.pointer !== event.pointerId) return
    gesture.current = null
    onTransformEnd()
  }

  const pointerEvents = {
    onPointerMove: update,
    onPointerUp: finish,
    onPointerCancel: finish,
    onLostPointerCapture: lostCapture,
  }

  return (
    <div
      data-testid='selection-controls'
      className='pointer-events-none absolute box-border border border-[var(--canvas-selection)]'
      style={{
        left: position.left,
        top: position.top,
        width: position.width,
        height: position.height,
        transform: `rotate(${position.angle}deg)`,
        transformOrigin: '50% 50%',
      }}
    >
      {handles.map(({ handle, left, top, cursor }) => {
        const horizontalEdge = handle === 'n' || handle === 's'
        const verticalEdge = handle === 'e' || handle === 'w'
        const isEdge = horizontalEdge || verticalEdge
        // Edge strips span the side between the corner targets, so a small frame
        // still leaves its corners reachable instead of being covered by the strips.
        const length = `max(${edgeHit}px, calc(100% - ${cornerHit}px))`
        return (
          <div
            key={handle}
            data-canvas-control
            data-resize-handle={handle}
            className='pointer-events-auto absolute grid -translate-x-1/2 -translate-y-1/2 touch-none place-items-center'
            style={{
              left,
              top,
              cursor,
              zIndex: isEdge ? 1 : 2,
              width: horizontalEdge ? length : isEdge ? edgeHit : cornerHit,
              height: verticalEdge ? length : isEdge ? edgeHit : cornerHit,
            }}
            onPointerDown={startResize(handle)}
            {...pointerEvents}
          >
            {!edgesOnly && (
              <span className='size-2 rounded-[1px] border border-primary-foreground bg-[var(--canvas-selection)] shadow-[0_0_0_1px_rgb(0_0_0/0.12)]' />
            )}
          </div>
        )
      })}

      <span className='pointer-events-none absolute top-0 left-1/2 h-2 w-px -translate-x-1/2 -translate-y-full bg-[var(--canvas-selection)]' />
      <div
        data-canvas-control
        data-rotate-handle
        className='pointer-events-auto absolute top-[-16px] left-1/2 z-[3] grid size-5 -translate-x-1/2 -translate-y-1/2 cursor-alias touch-none place-items-center'
        style={{ width: cornerHit, height: cornerHit }}
        onPointerDown={startRotate}
        {...pointerEvents}
      >
        <span className='size-2 rounded-[1px] border border-primary-foreground bg-[var(--canvas-selection)] shadow-[0_0_0_1px_rgb(0_0_0/0.12)]' />
      </div>

      {onRefit && (
        <button
          type='button'
          data-canvas-control
          data-testid='refit-text'
          className='pointer-events-auto absolute top-full left-1/2 z-[3] mt-2 -translate-x-1/2 rounded-md border border-border bg-popover px-2 py-0.5 text-xs whitespace-nowrap text-popover-foreground shadow-sm hover:bg-accent'
          onPointerDown={(event) => event.stopPropagation()}
          onClick={(event) => {
            event.stopPropagation()
            onRefit()
          }}
        >
          {refitLabel}
        </button>
      )}
    </div>
  )
}
