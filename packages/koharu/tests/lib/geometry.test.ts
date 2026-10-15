import { describe, expect, it } from 'vitest'

import { growFrame, nudgeFrame, resizeFrame, rotateFrame } from '@/lib/geometry'
import type { Frame } from '@koharu/bridge/protocol'

const frame: Frame = { x: 10, y: 20, width: 100, height: 50, angle_degrees: 0 }

describe('selection control geometry', () => {
  it('resizes from a handle while keeping the opposite edge fixed', () => {
    expect(resizeFrame(frame, 'e', { x: 140, y: 45 }, 1)).toEqual({
      ...frame,
      width: 130,
    })
    expect(resizeFrame(frame, 'nw', { x: 0, y: 10 }, 1)).toEqual({
      ...frame,
      x: 0,
      y: 10,
      width: 110,
      height: 60,
    })
  })

  it('resizes in the frame local axes when it is rotated', () => {
    const result = resizeFrame({ ...frame, angle_degrees: 90 }, 'e', { x: 60, y: 125 }, 1)
    expect(result.x).toBeCloseTo(-5)
    expect(result.y).toBeCloseTo(35)
    expect(result.width).toBeCloseTo(130)
    expect(result.height).toBe(50)
  })

  it('rotates around the frame center', () => {
    expect(rotateFrame(frame, { x: 60, y: -5 }, { x: 110, y: 45 })).toEqual({
      ...frame,
      angle_degrees: 90,
    })
  })

  it('keeps the aspect ratio with Shift', () => {
    const result = resizeFrame(frame, 'se', { x: 210, y: 30 }, 1, { keepAspect: true })
    expect(result.width / result.height).toBeCloseTo(2)
    expect(result.x).toBe(10)
    expect(result.y).toBe(20)
    expect(result.width).toBeCloseTo(200)
    const edge = resizeFrame(frame, 'e', { x: 160, y: 45 }, 1, { keepAspect: true })
    expect(edge.width).toBeCloseTo(150)
    expect(edge.height).toBeCloseTo(75)
  })

  it('resizes around the centre with Alt', () => {
    expect(resizeFrame(frame, 'e', { x: 140, y: 45 }, 1, { fromCenter: true })).toEqual({
      ...frame,
      x: -20,
      width: 160,
    })
  })

  it('nudges and grows frames in page units', () => {
    expect(nudgeFrame(frame, 1, -10)).toEqual({ ...frame, x: 11, y: 10 })
    expect(growFrame(frame, 10, -100, 1)).toEqual({
      ...frame,
      x: 5,
      y: 44.5,
      width: 110,
      height: 1,
    })
  })
})
