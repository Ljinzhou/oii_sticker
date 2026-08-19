export type AutoScrollDirection = 1 | -1;

export interface AutoScrollState {
  position: number;
  direction: AutoScrollDirection;
}

function positiveModulo(value: number, modulus: number): number {
  return ((value % modulus) + modulus) % modulus;
}

/** 在 0..maxPosition 之间按反射边界推进浮点滚动位置。 */
export function advanceAutoScroll(
  state: Readonly<AutoScrollState>,
  maxPosition: number,
  speed: number,
  deltaMs: number,
): AutoScrollState {
  if (!Number.isFinite(maxPosition) || maxPosition <= 0) {
    return { position: 0, direction: 1 };
  }

  const position = Number.isFinite(state.position)
    ? Math.min(maxPosition, Math.max(0, state.position))
    : 0;
  const direction: AutoScrollDirection = state.direction === -1 ? -1 : 1;
  if (!Number.isFinite(speed) || !Number.isFinite(deltaMs) || speed <= 0 || deltaMs <= 0) {
    return { position, direction };
  }

  const distance = speed * deltaMs / 1000;
  const period = maxPosition * 2;
  const phase = direction === 1 ? position : period - position;
  const nextPhase = positiveModulo(phase + distance, period);

  if (nextPhase === 0) return { position: 0, direction: 1 };
  if (nextPhase === maxPosition) return { position: maxPosition, direction: -1 };
  if (nextPhase < maxPosition) return { position: nextPhase, direction: 1 };
  return { position: period - nextPhase, direction: -1 };
}
